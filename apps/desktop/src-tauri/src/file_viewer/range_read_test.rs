use super::*;
use crate::pluralize::pluralize;

#[test]
fn clamp_offset_inside_ascii() {
    assert_eq!(clamp_utf16_offset_to_byte("hello world", 0), 0);
    assert_eq!(clamp_utf16_offset_to_byte("hello world", 5), 5);
    assert_eq!(clamp_utf16_offset_to_byte("hello world", 11), 11);
    // Past the end: clamps to byte length.
    assert_eq!(clamp_utf16_offset_to_byte("hello world", 99), 11);
}

#[test]
fn clamp_offset_in_surrogate_pair() {
    // "👋hello" — emoji is 2 UTF-16 units (a high + low surrogate) and 4 UTF-8 bytes.
    let s = "👋hello";
    assert_eq!(clamp_utf16_offset_to_byte(s, 0), 0);
    // Offset 1: lands inside the surrogate pair; clamp down to codepoint start (0).
    assert_eq!(clamp_utf16_offset_to_byte(s, 1), 0);
    // Offset 2: end of the emoji, start of 'h' (byte 4).
    assert_eq!(clamp_utf16_offset_to_byte(s, 2), 4);
    // Offset 3: end of 'h' (byte 5).
    assert_eq!(clamp_utf16_offset_to_byte(s, 3), 5);
}

#[test]
fn clamp_offset_in_multi_byte_utf8_but_single_utf16() {
    // "café" — 'é' is 2 UTF-8 bytes but 1 UTF-16 unit.
    let s = "café";
    assert_eq!(clamp_utf16_offset_to_byte(s, 0), 0);
    assert_eq!(clamp_utf16_offset_to_byte(s, 1), 1);
    assert_eq!(clamp_utf16_offset_to_byte(s, 2), 2);
    assert_eq!(clamp_utf16_offset_to_byte(s, 3), 3); // start of 'é'
    assert_eq!(clamp_utf16_offset_to_byte(s, 4), 5); // end of 'é', byte 5
}

/// The streamed read's peak memory is one chunk, whatever the range's size: no
/// piece handed to the sink exceeds the chunk budget plus the line that crossed it.
#[test]
fn streamed_read_hands_out_bounded_pieces() {
    // 20 MB of range through a 64 KiB budget.
    let line_len = 999;
    let line_count = 20_000;
    let backend = crate::file_viewer::session::ScriptedBackend::new(&"z".repeat(line_len), line_count, |_| {});
    let chunk_bytes = 64 * 1024;

    let mut max_piece = 0usize;
    let mut total = 0usize;
    let mut sink = |piece: &[u8]| {
        max_piece = max_piece.max(piece.len());
        total += piece.len();
        Ok(())
    };
    read_range_streamed(
        &backend,
        RangeEnd::Row { row: 0, offset: 0 },
        RangeEnd::Eof,
        &AtomicBool::new(false),
        chunk_bytes,
        RowForm::Text,
        &mut sink,
    )
    .unwrap();

    // Every line plus its joining newline, minus the newline past the last line.
    assert_eq!(total, line_count * (line_len + 1) - 1);
    assert!(
        max_piece <= chunk_bytes + line_len + 1,
        "peak piece was {} against a {chunk_bytes}-byte budget",
        pluralize(max_piece as u64, "byte")
    );
}

#[test]
fn compare_ends_orders_eof_greatest() {
    let a = RangeEnd::Row { row: 5, offset: 3 };
    let b = RangeEnd::Row { row: 5, offset: 7 };
    let c = RangeEnd::Eof;
    assert!(compare_ends(&a, &b).is_lt());
    assert!(compare_ends(&b, &a).is_gt());
    assert!(compare_ends(&a, &c).is_lt());
    assert!(compare_ends(&c, &c).is_eq());
}

/// The one row `raw` holds, decoded the way a backend decodes it.
fn row_in(raw: &[u8], encoding: FileEncoding) -> ViewerRow {
    let text = raw.strip_suffix(encoding.newline_bytes()).unwrap_or(raw);
    ViewerRow {
        text: decode_line(text, encoding),
        byte_offset: 0,
        continues: false,
        line_number: Some(0),
    }
}

fn row_of(raw: &[u8]) -> ViewerRow {
    row_in(raw, FileEncoding::Utf8)
}

/// What `form` writes for `row.text[lo..hi]`, the row's raw bytes being `raw`.
fn piece_of(form: RowForm<'_>, raw: &[u8], encoding: FileEncoding, lo: usize, hi: usize) -> Result<Vec<u8>, Walked> {
    let row = row_in(raw, encoding);
    match form.piece(&row, raw.len() as u64, Some((0, raw)), lo, hi).unwrap() {
        Piece::Row { body, .. } => Ok(body.into_owned()),
        Piece::Stop(walked) => Err(walked),
    }
}

/// `row.text[lo..hi]` as a UTF-8 save writes it, the row's raw bytes being `raw`.
fn saved(raw: &[u8], lo: usize, hi: usize) -> Vec<u8> {
    let read = |start: u64, end: u64| Ok(raw[start as usize..end as usize].to_vec());
    let source = RawSource {
        read: &read,
        encoding: FileEncoding::Utf8,
    };
    piece_of(RowForm::TextKeepingLoss(source), raw, FileEncoding::Utf8, lo, hi).unwrap()
}

/// Selection edges on either side of a replaced byte map to the bytes around it,
/// with nothing dropped or doubled, however the invalid run is shaped.
#[test]
fn a_replaced_row_saves_its_own_bytes_between_the_selection_edges() {
    // "caf" + one U+FFFD (3 decoded bytes) for 0xE9 + " ok", CRLF-ended.
    let raw = b"caf\xE9 ok\r\n";
    let decoded_len = row_of(raw).text.len();
    assert_eq!(saved(raw, 0, decoded_len), b"caf\xE9 ok\r");
    assert_eq!(saved(raw, 3, 6), b"\xE9", "exactly the replaced byte");
    assert_eq!(saved(raw, 0, 3), b"caf", "up to the replaced byte, not into it");
    assert_eq!(saved(raw, 6, 9), b" ok", "from just past it");

    // A multibyte char cut short (0xE2 0x82, no third byte), then a lone 0xFF: two
    // invalid runs, two U+FFFD, back to back.
    let raw = b"a\xE2\x82\xFFb\n";
    assert_eq!(row_of(raw).text, "a\u{FFFD}\u{FFFD}b");
    assert_eq!(saved(raw, 1, 4), b"\xE2\x82", "the first run alone");
    assert_eq!(saved(raw, 4, 7), b"\xFF", "the second run alone");
    assert_eq!(saved(raw, 0, 8), b"a\xE2\x82\xFFb");
    // A valid multibyte char beside an invalid run keeps its bytes whole.
    let raw = b"\xE2\x82\xAC\xE9\xC3\xA9\n";
    assert_eq!(row_of(raw).text, "\u{20AC}\u{FFFD}\u{E9}");
    assert_eq!(saved(raw, 0, 3), b"\xE2\x82\xAC");
    assert_eq!(saved(raw, 6, 8), b"\xC3\xA9");
}

/// A file that holds a real U+FFFD has nothing to restore: its bytes ARE the text.
#[test]
fn a_real_replacement_character_saves_as_decoded() {
    let raw = "x\u{FFFD}y\n".as_bytes();
    assert_eq!(saved(raw, 0, 5), "x\u{FFFD}y".as_bytes());
}

/// Raw bytes that don't decode to the row on screen (the file changed under a
/// backend that holds it in memory) can't be mapped onto the selection, so the save
/// keeps the decoded text rather than splice in bytes from the wrong place.
#[test]
fn raw_bytes_that_no_longer_match_the_row_save_as_decoded() {
    let row = row_of(b"caf\xE9 au lait\n");
    let changed = b"XYZ\xFFwholly different\n".to_vec();
    let read = move |_: u64, _: u64| Ok(changed.clone());
    let source = RawSource {
        read: &read,
        encoding: FileEncoding::Utf8,
    };
    let Piece::Row { body, .. } = RowForm::TextKeepingLoss(source)
        .piece(&row, 15, None, 0, row.text.len())
        .unwrap()
    else {
        panic!("a UTF-8 row never stops the walk");
    };
    assert_eq!(&*body, row.text.as_bytes());
}

/// `text` as UTF-16 LE, with `0xD800` (a lone high surrogate, which no decode can
/// read) wherever `text` has a `|`.
fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .map(|unit| if unit == u16::from(b'|') { 0xD800 } else { unit })
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// Every character boundary of the decoded text maps to the source bytes that decode
/// to exactly the text before it: the property the save's selection edges stand on.
#[test]
fn raw_index_lands_where_the_decoded_prefix_ends() {
    let cases: Vec<(Vec<u8>, FileEncoding)> = vec![
        (b"caf\xE9 ok\r".to_vec(), FileEncoding::Utf8),
        (b"a\xE2\x82\xFFb".to_vec(), FileEncoding::Utf8),
        (utf16le("be|ta"), FileEncoding::Utf16Le),
        (utf16le("||x"), FileEncoding::Utf16Le),
        (utf16le("a\u{1F44B}b|"), FileEncoding::Utf16Le),
        // A lone high surrogate right before a valid pair: the pair survives whole.
        ([0x00, 0xD8, 0x3D, 0xD8, 0x4B, 0xDC].to_vec(), FileEncoding::Utf16Le),
        // An odd byte at the end of a UTF-16 run.
        ([0x41, 0x00, 0x42].to_vec(), FileEncoding::Utf16Le),
        ([0x00, 0x41, 0xD8, 0x00, 0x00, 0x42].to_vec(), FileEncoding::Utf16Be),
        (b"\x80caf\xE9\x81".to_vec(), FileEncoding::Windows1252),
        (b"\x80caf\xE9\x81".to_vec(), FileEncoding::Iso8859_1),
        (b"\x80caf\x8E".to_vec(), FileEncoding::MacRoman),
    ];
    for (raw, encoding) in cases {
        let text = decode_line(&raw, encoding);
        let mut last = 0;
        for (decoded, _) in text.char_indices().chain([(text.len(), ' ')]) {
            let at = raw_index(&raw, decoded, encoding);
            assert!(at >= last, "{encoding:?} {raw:x?}: went backwards at {decoded}");
            assert_eq!(
                decode_line(&raw[..at], encoding),
                text[..decoded],
                "{encoding:?} {raw:x?}: decoded byte {decoded} mapped to source byte {at}"
            );
            last = at;
        }
        assert_eq!(raw_index(&raw, text.len(), encoding), raw.len());
    }
}

/// A UTF-16 selection holding a character the decode lost stops the text walk, so
/// the save can start over in the source's encoding; one holding a real U+FFFD
/// doesn't.
#[test]
fn a_utf16_selection_that_lost_a_character_asks_for_the_source_form() {
    let raw = utf16le("be|ta\n");
    let read = |start: u64, end: u64| Ok(raw[start as usize..end as usize].to_vec());
    let source = RawSource {
        read: &read,
        encoding: FileEncoding::Utf16Le,
    };
    let form = RowForm::TextKeepingLoss(source);
    // "be" + U+FFFD + "ta": the lost character is decoded bytes 2..5.
    assert_eq!(piece_of(form, &raw, FileEncoding::Utf16Le, 0, 5), Err(Walked::Lossy));
    assert_eq!(piece_of(form, &raw, FileEncoding::Utf16Le, 5, 7), Ok(b"ta".to_vec()));

    let real = "x\u{FFFD}y\n"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let read = |start: u64, end: u64| Ok(real[start as usize..end as usize].to_vec());
    let source = RawSource {
        read: &read,
        encoding: FileEncoding::Utf16Le,
    };
    let piece = piece_of(RowForm::TextKeepingLoss(source), &real, FileEncoding::Utf16Le, 0, 5);
    assert_eq!(piece, Ok("x\u{FFFD}y".as_bytes().to_vec()));
}

/// The source form writes the row's own bytes, and its newline is the source's.
#[test]
fn the_source_form_writes_the_rows_own_bytes() {
    let raw = utf16le("be|ta\n");
    let read = |_: u64, _: u64| Ok(Vec::new()); // unused: the chunk's bytes come along
    let source = RawSource {
        read: &read,
        encoding: FileEncoding::Utf16Le,
    };
    let row = row_in(&raw, FileEncoding::Utf16Le);
    let Piece::Row { body, newline } = RowForm::Source(source)
        .piece(&row, raw.len() as u64, Some((0, &raw)), 1, 7)
        .unwrap()
    else {
        panic!("the source matches the row");
    };
    assert_eq!(&*body, &utf16le("e|ta")[..]);
    assert_eq!(newline, b"\n\0");
}

/// Source bytes that don't decode to the row the walk holds end the source walk, so
/// the save falls back to the decoded text instead of writing someone else's bytes.
#[test]
fn the_source_form_stops_on_bytes_that_no_longer_match_the_row() {
    let row = row_in(&utf16le("be|ta\n"), FileEncoding::Utf16Le);
    let changed = utf16le("wholly different\n");
    let read = |_: u64, _: u64| Ok(Vec::new());
    let source = RawSource {
        read: &read,
        encoding: FileEncoding::Utf16Le,
    };
    let piece = RowForm::Source(source)
        .piece(&row, changed.len() as u64, Some((0, &changed)), 0, row.text.len())
        .unwrap();
    assert!(matches!(piece, Piece::Stop(Walked::SourceChanged)));
}

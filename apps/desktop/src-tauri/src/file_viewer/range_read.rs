//! Stitches a `(line, offset)` -> `(line, offset)` range read into UTF-8 text,
//! independent of which backend the session uses.
//!
//! `read_range_streamed` is the engine: it hands the range to a sink in bounded pieces,
//! so the save-to-file path never holds more than one chunk. `read_range` is the same
//! read with a sink that collects, for callers (the clipboard) that want one string.
//!
//! Offsets on the wire are UTF-16 code units (matches JS string indexing and the search
//! engine's `SearchMatch.column`). Conversion to UTF-8 byte positions happens here, at
//! the IPC boundary, via `clamp_utf16_offset_to_byte`. Lone surrogates (offsets that land
//! between the high and low surrogate of an astral codepoint) are clamped down to the
//! nearest codepoint boundary, so the output is always valid UTF-8.
//!
//! Range semantics are half-open `[start, end)`, matching the frontend selection model:
//! the start line is included from `start.offset` to its end, intermediate lines are
//! included in full (with their trailing newline), the end line is included from offset 0
//! up to but not including `end.offset`.
//!
//! Cancellation: the reader checks the cancel flag periodically (after each line in the
//! current implementation; for very long lines we'd need a finer-grained check, but the
//! backends already cap line length implicitly through `MAX_BACKWARD_SCAN`). When the
//! flag is set, the function returns `ViewerError::Cancelled`.

use std::borrow::Cow;
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicBool, Ordering};

use encoding_rs::DecoderResult;
use serde::Deserialize;

use super::encoding::{FileEncoding, decode_line, decodes_cleanly};
use super::{ChunkEnd, FileViewerBackend, SeekTarget, ViewerError, ViewerRow};

/// One endpoint of a selection: a ROW index plus a UTF-16 offset into that row. For the
/// "select all" path in ByteSeek-no-index mode (where the row count is unknown), the
/// frontend sends `Eof` so the backend can resolve the end without a fake row number.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RangeEnd {
    Row { row: u64, offset: u32 },
    Eof,
}

impl RangeEnd {
    /// True if this endpoint is `Eof`.
    fn is_eof(&self) -> bool {
        matches!(self, Self::Eof)
    }
}

/// Compares two endpoints under the assumption that `Eof` is greater than every
/// `Row { ... }`. Returns `std::cmp::Ordering`.
fn compare_ends(a: &RangeEnd, b: &RangeEnd) -> std::cmp::Ordering {
    use std::cmp::Ordering as O;
    match (a, b) {
        (RangeEnd::Eof, RangeEnd::Eof) => O::Equal,
        (RangeEnd::Eof, _) => O::Greater,
        (_, RangeEnd::Eof) => O::Less,
        (RangeEnd::Row { row: la, offset: oa }, RangeEnd::Row { row: lb, offset: ob }) => {
            la.cmp(lb).then_with(|| oa.cmp(ob))
        }
    }
}

/// Returns the byte index inside `line` corresponding to the given UTF-16 code-unit
/// offset, clamping down to the nearest codepoint boundary if the offset lands between
/// the high and low surrogate of an astral codepoint.
///
/// For an offset >= the line's total UTF-16 length, returns `line.len()` (byte length).
pub fn clamp_utf16_offset_to_byte(line: &str, utf16_offset: u32) -> usize {
    let target = utf16_offset as usize;
    let mut utf16_count: usize = 0;
    for (byte_idx, ch) in line.char_indices() {
        if utf16_count >= target {
            return byte_idx;
        }
        utf16_count += ch.len_utf16();
        if utf16_count > target {
            // The offset landed inside a surrogate pair; clamp down to the codepoint
            // start (which is `byte_idx`).
            return byte_idx;
        }
    }
    line.len()
}

/// How much range text the streaming reader holds before handing it to its sink.
///
/// This is the save path's peak allocation, whatever the selection's size: the copy
/// dialog refuses a clipboard copy past 100 MiB and offers "Save as" as the way out, so
/// the save must not allocate the very thing the refusal protects the user from. 1 MiB
/// stays out of the way on any machine, and still costs a multi-GB save only a few
/// thousand writes.
pub(crate) const STREAM_CHUNK_BYTES: usize = 1024 * 1024;

/// Collects range text and hands it to `sink` in pieces of at most `chunk_bytes` plus
/// the tail of the line that crossed the threshold.
///
/// The newline that joins two lines (`\n`, or the source's own in [`RowForm::Source`])
/// is held back until the next line arrives, so the range's final newline (which
/// half-open semantics drop) never has to be taken back out of a piece that already
/// left for the sink.
struct ChunkedSink<'a, S: FnMut(&[u8]) -> Result<(), ViewerError>> {
    buf: Vec<u8>,
    chunk_bytes: usize,
    sink: &'a mut S,
    newline_owed: Option<&'static [u8]>,
}

impl<'a, S: FnMut(&[u8]) -> Result<(), ViewerError>> ChunkedSink<'a, S> {
    fn new(sink: &'a mut S, chunk_bytes: usize) -> Self {
        Self {
            buf: Vec::new(),
            chunk_bytes,
            sink,
            newline_owed: None,
        }
    }

    /// Appends `bytes`, first paying any newline owed to the previous line.
    fn push(&mut self, bytes: &[u8]) -> Result<(), ViewerError> {
        self.pay_newline();
        self.buf.extend_from_slice(bytes);
        if self.buf.len() >= self.chunk_bytes {
            self.flush()?;
        }
        Ok(())
    }

    /// Ends a line: its `newline` lands only once something follows it.
    fn end_line(&mut self, newline: &'static [u8]) {
        self.newline_owed = Some(newline);
    }

    fn pay_newline(&mut self) {
        if let Some(newline) = self.newline_owed.take() {
            self.buf.extend_from_slice(newline);
        }
    }

    /// Hands what's left to the sink, ending the walk. `keep_trailing_newline` pays a
    /// newline still owed, which only the "ran past the end line" exit wants.
    fn finish(mut self, keep_trailing_newline: bool) -> Result<Walked, ViewerError> {
        if keep_trailing_newline {
            self.pay_newline();
        }
        self.flush()?;
        Ok(Walked::Finished)
    }

    fn flush(&mut self) -> Result<(), ViewerError> {
        if !self.buf.is_empty() {
            (self.sink)(&self.buf)?;
            self.buf.clear();
        }
        Ok(())
    }
}

/// Reads the selected range from the given backend, returning a single UTF-8 string.
///
/// Holds the whole range in memory by definition; a caller that only wants to put the
/// range somewhere (the save-to-file path) uses [`read_range_streamed`] instead.
pub fn read_range(
    backend: &dyn FileViewerBackend,
    anchor: RangeEnd,
    focus: RangeEnd,
    cancel: &AtomicBool,
) -> Result<String, ViewerError> {
    let mut out = Vec::new();
    let mut sink = |piece: &[u8]| {
        out.extend_from_slice(piece);
        Ok(())
    };
    // `Text` never stops early, so the walk always answers `Finished`.
    let walked = read_range_streamed(
        backend,
        anchor,
        focus,
        cancel,
        STREAM_CHUNK_BYTES,
        RowForm::Text,
        &mut sink,
    )?;
    debug_assert_eq!(walked, Walked::Finished);
    // Decoded rows only, so the pieces are UTF-8 by construction.
    String::from_utf8(out).map_err(|e| ViewerError::Io {
        message: e.to_string().into(),
    })
}

/// Reads the source's own bytes in `[start, end)`.
pub(crate) type ReadSource<'a> = &'a dyn Fn(u64, u64) -> Result<Vec<u8>, ViewerError>;

/// Where a save reads the source's own bytes, and the encoding they're in.
#[derive(Clone, Copy)]
pub(crate) struct RawSource<'a> {
    pub read: ReadSource<'a>,
    pub encoding: FileEncoding,
}

/// What the walk writes for each row.
#[derive(Clone, Copy)]
pub(crate) enum RowForm<'a> {
    /// The decoded text, as UTF-8: the clipboard, and a save's fallback.
    Text,
    /// The decoded text, watching the selection for characters the decode lost. A
    /// UTF-8-family row that lost some writes its own bytes instead (clean text IS its
    /// bytes, so the output stays one encoding); any other source stops the walk with
    /// [`Walked::Lossy`], so the save can start over in [`RowForm::Source`].
    TextKeepingLoss(RawSource<'a>),
    /// Every row's own bytes: the range exactly as the source holds it.
    Source(RawSource<'a>),
}

/// How a walk ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Walked {
    Finished,
    /// [`RowForm::TextKeepingLoss`] met a selected character the decode lost, in a
    /// source whose bytes aren't UTF-8.
    Lossy,
    /// [`RowForm::Source`] read bytes that don't decode to the rows the walk holds: the
    /// file changed under a backend that keeps it in memory.
    SourceChanged,
}

/// What the walk writes for one row: the selected part, and the newline it owes the
/// next row (if one follows).
enum Piece<'r> {
    Row {
        body: Cow<'r, [u8]>,
        newline: &'static [u8],
    },
    Stop(Walked),
}

impl RowForm<'_> {
    /// The piece for `row.text[lo..hi]`. `row_end` is where the row's bytes stop (its
    /// newline included); `chunk_raw` is the source's bytes from `.0` on, which only
    /// [`RowForm::Source`] reads, once per chunk.
    fn piece<'r>(
        &self,
        row: &'r ViewerRow,
        row_end: u64,
        chunk_raw: Option<(u64, &'r [u8])>,
        lo: usize,
        hi: usize,
    ) -> Result<Piece<'r>, ViewerError> {
        let text = Piece::Row {
            body: Cow::Borrowed(&row.text.as_bytes()[lo..hi]),
            newline: b"\n",
        };
        match *self {
            Self::Text => Ok(text),
            Self::TextKeepingLoss(source) => {
                if !row.text[lo..hi].contains(char::REPLACEMENT_CHARACTER) {
                    return Ok(text);
                }
                let raw = (source.read)(row.byte_offset, row_end)?;
                let encoding = source.encoding;
                // The mapping holds only for the bytes the row was decoded from. A file
                // that changed under a backend holding it in memory reads back other
                // ones, which would splice bytes from the wrong place into the selection.
                let Some((body, _)) = row_body(row, &raw, encoding) else {
                    return Ok(text);
                };
                let selected = select(body, &row.text, lo, hi, encoding);
                if decodes_cleanly(selected, encoding) {
                    return Ok(text); // the file holds a real U+FFFD
                }
                if !encoding.is_utf8_family() {
                    return Ok(Piece::Stop(Walked::Lossy));
                }
                Ok(Piece::Row {
                    body: Cow::Owned(selected.to_vec()),
                    newline: b"\n",
                })
            }
            Self::Source(source) => {
                let raw = chunk_raw.and_then(|(base, raw)| {
                    let from = usize::try_from(row.byte_offset.checked_sub(base)?).ok()?;
                    let to = usize::try_from(row_end.checked_sub(base)?).ok()?;
                    raw.get(from..to)
                });
                let Some((body, newline)) = raw.and_then(|raw| row_body(row, raw, source.encoding)) else {
                    return Ok(Piece::Stop(Walked::SourceChanged));
                };
                Ok(Piece::Row {
                    body: Cow::Borrowed(select(body, &row.text, lo, hi, source.encoding)),
                    newline,
                })
            }
        }
    }

    /// The source's bytes in `[start, end)` when this form writes them for every row.
    fn chunk_raw(&self, start: u64, end: u64) -> Result<Option<Vec<u8>>, ViewerError> {
        match self {
            Self::Source(source) => (source.read)(start, end).map(Some),
            Self::Text | Self::TextKeepingLoss(_) => Ok(None),
        }
    }
}

/// A row's own bytes `raw` split into its text and its newline, or `None` when the text
/// part doesn't decode to the row the walk holds.
fn row_body<'b>(row: &ViewerRow, raw: &'b [u8], encoding: FileEncoding) -> Option<(&'b [u8], &'static [u8])> {
    let newline = encoding.newline_bytes();
    let (body, newline) = match raw.strip_suffix(newline) {
        Some(body) if !row.continues => (body, newline),
        _ => (raw, &b""[..]),
    };
    let matches = body == row.text.as_bytes() || decode_line(body, encoding) == row.text;
    matches.then_some((body, newline))
}

/// The bytes of `body` (a row's text part, which decodes to `text`) that decode to
/// `text[lo..hi]`.
fn select<'b>(body: &'b [u8], text: &str, lo: usize, hi: usize, encoding: FileEncoding) -> &'b [u8] {
    let at = |decoded: usize| match decoded {
        0 => 0,
        d if d == text.len() => body.len(),
        d => raw_index(body, d, encoding),
    };
    &body[at(lo)..at(hi)]
}

/// Where byte `decoded` of `decode_line(raw, encoding)` sits in `raw`. The decode swaps
/// each malformed run for one U+FFFD and spends a fixed number of bytes on every other
/// character in the encodings the viewer offers, which is the whole mapping; `decoded`
/// is a char boundary, so it never lands inside one.
fn raw_index(raw: &[u8], decoded: usize, encoding: FileEncoding) -> usize {
    let (mut at_decoded, mut at_raw) = (0, 0);
    let walk = for_each_source_char(raw, encoding, |decoded_len, raw_len| {
        if decoded <= at_decoded {
            return ControlFlow::Break(());
        }
        at_decoded += decoded_len;
        at_raw += raw_len;
        ControlFlow::Continue(())
    });
    match walk {
        ControlFlow::Break(()) => at_raw,       // stopped on the char that starts at `decoded`
        ControlFlow::Continue(()) => raw.len(), // `decoded` is the end
    }
}

/// Calls `f(decoded_len, source_len)` for each character `decode_line(raw, encoding)`
/// yields, in order, a U+FFFD for a malformed run counting that run's bytes.
fn for_each_source_char(
    raw: &[u8],
    encoding: FileEncoding,
    mut f: impl FnMut(usize, usize) -> ControlFlow<()>,
) -> ControlFlow<()> {
    const FFFD_LEN: usize = char::REPLACEMENT_CHARACTER.len_utf8();
    if encoding.is_utf8_family() {
        // `from_utf8_lossy` swaps each invalid run `utf8_chunks` reports for one U+FFFD.
        for chunk in raw.utf8_chunks() {
            for ch in chunk.valid().chars() {
                f(ch.len_utf8(), ch.len_utf8())?;
            }
            if !chunk.invalid().is_empty() {
                f(FFFD_LEN, chunk.invalid().len())?;
            }
        }
        return ControlFlow::Continue(());
    }
    if matches!(encoding, FileEncoding::Iso8859_1) {
        for &byte in raw {
            f(char::from(byte).len_utf8(), 1)?;
        }
        return ControlFlow::Continue(());
    }
    let width = |ch: char| match encoding {
        FileEncoding::Utf16Le | FileEncoding::Utf16Be => ch.len_utf16() * 2,
        FileEncoding::Windows1252 | FileEncoding::Iso8859_1 | FileEncoding::MacRoman => 1,
        FileEncoding::Utf8 | FileEncoding::Utf8WithBom | FileEncoding::UsAscii => ch.len_utf8(),
    };
    // Decode a clean run at a time. A malformed report names the bad run's length and
    // how many bytes past it the decoder already took (a UTF-16 unit after a lone high
    // surrogate); those are re-read by a fresh decoder, which is what the replacing
    // decode makes of them too.
    let mut at = 0;
    let mut run = String::new();
    loop {
        let rest = &raw[at..];
        let mut decoder = encoding.as_static().new_decoder_without_bom_handling();
        run.clear();
        run.reserve(
            decoder
                .max_utf8_buffer_length_without_replacement(rest.len())
                .unwrap_or(rest.len() * 3 + 4),
        );
        let (result, read) = decoder.decode_to_string_without_replacement(rest, &mut run, true);
        for ch in run.chars() {
            f(ch.len_utf8(), width(ch))?;
        }
        match result {
            DecoderResult::Malformed(bad, past) => {
                f(FFFD_LEN, usize::from(bad))?;
                let advance = read.saturating_sub(usize::from(past));
                if advance == 0 {
                    return ControlFlow::Continue(()); // can't happen: a bad run is at least one byte
                }
                at += advance;
            }
            // The reserve fits the whole decode, so a full buffer can't happen; stop
            // rather than loop if it ever does.
            DecoderResult::InputEmpty | DecoderResult::OutputFull => return ControlFlow::Continue(()),
        }
    }
}

/// Reads the selected range and hands it to `sink` in pieces of at most `chunk_bytes`
/// (plus the tail of the line that crossed the threshold), so a caller that writes the
/// pieces straight out never holds more than one chunk.
///
/// Endpoints are normalised internally; reversed input (focus before anchor) produces
/// the same output as the forward range.
///
/// Returns `ViewerError::Cancelled` if `cancel` is flipped during the read, and
/// `ViewerError::OutOfRange` if the requested line is past the file's last line (with
/// the exception that `Eof` is always valid). A `sink` that fails stops the read with
/// its own error, and nothing further is read. `form` says what each row writes; a walk
/// that stops early says why in its [`Walked`], having handed over only part of the range.
///
/// Streaming: after the initial seek by line number, the function advances by **byte
/// offset** rather than line number. This is mandatory for the ByteSeek backend, which
/// only estimates line numbers (`SeekTarget::Row(N)` resolves to `N * 80` bytes); for
/// FullLoad and LineIndex backends, byte-offset seeking is equally well-supported and
/// gives a single code path.
pub fn read_range_streamed<S: FnMut(&[u8]) -> Result<(), ViewerError>>(
    backend: &dyn FileViewerBackend,
    anchor: RangeEnd,
    focus: RangeEnd,
    cancel: &AtomicBool,
    chunk_bytes: usize,
    form: RowForm<'_>,
    sink: &mut S,
) -> Result<Walked, ViewerError> {
    let (start, end) = if compare_ends(&anchor, &focus).is_le() {
        (anchor, focus)
    } else {
        (focus, anchor)
    };

    // Resolve start row + offset. `Eof` as the start is unusual but well-defined:
    // empty selection at end of file.
    let (start_row, start_offset_utf16) = match start {
        RangeEnd::Row { row, offset } => (row as usize, offset),
        RangeEnd::Eof => return Ok(Walked::Finished),
    };

    // Validate the start row against the backend's row count. Only an EXACT count can
    // refuse a read: `ByteSeekBackend` estimates, and refusing on an estimate would
    // turn a copy into an error on a file it could have served.
    if backend.total_rows().is_exact() && start_row >= backend.total_rows().rows() {
        return Err(ViewerError::OutOfRange);
    }

    // Resolve end. `Eof` means "to the last line, all of it"; otherwise we have an
    // explicit `Row { row, offset }`.
    let end_is_eof = end.is_eof();
    let (end_row, end_offset_utf16) = match end {
        RangeEnd::Row { row, offset } => (row as usize, offset),
        RangeEnd::Eof => (usize::MAX, 0),
    };

    let mut emit = ChunkedSink::new(sink, chunk_bytes);

    if start_row == end_row && !end_is_eof {
        // Single-row read: fetch the one row, clamp both offsets, slice between them.
        let chunk = backend.get_lines(&SeekTarget::Row(start_row), 1, cancel)?;
        let row = chunk.rows.first().ok_or(ViewerError::OutOfRange)?;
        let start_byte = clamp_utf16_offset_to_byte(&row.text, start_offset_utf16);
        let end_byte = clamp_utf16_offset_to_byte(&row.text, end_offset_utf16);
        let lo = start_byte.min(end_byte);
        let hi = start_byte.max(end_byte);
        let raw = form.chunk_raw(row.byte_offset, chunk.end_byte_offset)?;
        let chunk_raw = raw.as_deref().map(|raw| (row.byte_offset, raw));
        match form.piece(row, chunk.end_byte_offset, chunk_raw, lo, hi)? {
            Piece::Row { body, .. } => emit.push(&body)?,
            Piece::Stop(walked) => return Ok(walked),
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(ViewerError::Cancelled);
        }
        return emit.finish(/*keep_trailing_newline=*/ false);
    }

    // Multi-row streaming read. First chunk is keyed by start row (only call that
    // uses `SeekTarget::Row` so we land on the right starting row). Subsequent chunks
    // are keyed by **byte offset** of the end of the last chunk, which is exact for all
    // three backends (ByteSeek's `Row(N)` is approximate; its byte-offset seeks are
    // exact, just back-scan for the surrounding newline).
    const FETCH_CHUNK: usize = 4096;
    // Cancellation budget inside the per-line loop. The plan's "every 64 KB" was the
    // target; we check whichever lands first: 256 lines (cheap line counter) or 64 KB
    // of emitted text (cheap byte counter). At typical 80-byte lines that's a check
    // every 20 KB; at 4 KB-per-line files (which would dwarf the 256-line cap) we'd
    // check every ~16 lines. Either way the worst-case latency between Escape and
    // `Cancelled` returning is well under the 100 ms threshold for "feels responsive."
    const CANCEL_CHECK_LINES: usize = 256;
    const CANCEL_CHECK_BYTES: usize = 64 * 1024;
    let mut next_target = SeekTarget::Row(start_row);
    let mut first_chunk = true;
    // The row the walk is about to emit, counted forward from the first chunk.
    let mut row_number = start_row;
    let mut emitted_none_yet = true;
    let mut lines_since_cancel_check: usize = 0;
    let mut bytes_since_cancel_check: usize = 0;

    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(ViewerError::Cancelled);
        }

        let chunk = backend.get_lines(&next_target, FETCH_CHUNK, cancel)?;
        if chunk.rows.is_empty() {
            break;
        }

        // The chunk's own true source end offset.
        //
        // ❗ ❌ Never summed from decoded string lengths. Those are UTF-8 even when the
        // file is UTF-16, so a multi-chunk range over a UTF-16 file drifted a little
        // further with every chunk; and a `+ 1` per entry assumes every entry ended at
        // a newline, which a row that Cmdr broke did not. The backend knows where the
        // bytes stopped, so it says so. (CRLF still needs no special case: the readers
        // keep the `\r` in the row's text and the `\n` inside the row's span.)
        let chunk_end_offset = chunk.end_byte_offset;

        // ❗ The walk counts rows itself from the row it started on, rather than taking
        // each chunk's `first_row_number`. A backend without an index reports the row it
        // was ASKED for on a row-target seek but re-derives the number from its
        // bytes-per-row estimate on the byte-offset continuation chunks, so the
        // numbering jumps at a chunk seam on any file whose rows aren't uniform. Since
        // this number is what decides where an explicit-end range STOPS, a jump there
        // ends the copy on the wrong row. Counting is exact on every backend, because a
        // chunk's rows are contiguous by construction.
        if first_chunk {
            row_number = chunk.first_row_number;
        }

        // One read per chunk, sliced per row, for a form that writes every row's bytes.
        let chunk_start = chunk.rows.first().map_or(chunk_end_offset, |row| row.byte_offset);
        let raw = form.chunk_raw(chunk_start, chunk_end_offset)?;
        let chunk_raw = raw.as_deref().map(|raw| (chunk_start, raw));

        for (index, row) in chunk.rows.iter().enumerate() {
            let line = &row.text;
            let row_end = chunk
                .rows
                .get(index + 1)
                .map_or(chunk_end_offset, |next| next.byte_offset);
            // Check the cancel flag periodically inside the inner loop. Doing it only
            // between chunks meant a single 4096-line chunk of 4 KB/line files (16 MB)
            // was uninterruptible. Now Escape lands within ~64 KB of emitted output.
            if lines_since_cancel_check >= CANCEL_CHECK_LINES || bytes_since_cancel_check >= CANCEL_CHECK_BYTES {
                if cancel.load(Ordering::Relaxed) {
                    return Err(ViewerError::Cancelled);
                }
                lines_since_cancel_check = 0;
                bytes_since_cancel_check = 0;
            }

            let line_number = row_number;
            row_number += 1;
            let is_first_overall = emitted_none_yet;
            emitted_none_yet = false;

            // For explicit-end ranges, stop past the end line. The newline owed to the
            // last line emitted is part of the range here, so it's paid out.
            if !end_is_eof && line_number > end_row {
                return emit.finish(/*keep_trailing_newline=*/ true);
            }

            let from = if is_first_overall {
                // First line of the whole selection: take from start_offset to end of line.
                clamp_utf16_offset_to_byte(line, start_offset_utf16)
            } else if !end_is_eof && line_number == end_row {
                // Last line of an explicit range: take from offset 0 up to end_offset, and
                // no trailing delimiter (the range is half-open). Exits the walk here, so
                // it never reaches the `end_line()` below.
                let end_byte = clamp_utf16_offset_to_byte(line, end_offset_utf16);
                match form.piece(row, row_end, chunk_raw, 0, end_byte)? {
                    Piece::Row { body, .. } => emit.push(&body)?,
                    Piece::Stop(walked) => return Ok(walked),
                }
                return emit.finish(/*keep_trailing_newline=*/ false);
            } else {
                0
            };
            let (body, newline) = match form.piece(row, row_end, chunk_raw, from, line.len())? {
                Piece::Row { body, newline } => (body, newline),
                Piece::Stop(walked) => return Ok(walked),
            };
            emit.push(&body)?;
            // The one place the walk decides a row carries its delimiter, and the one
            // line that had to change for rows: ❗ a break Cmdr made is NOT a newline,
            // so joining a continuing row to the next would put a line break in the
            // clipboard and in save-as that the file never contained.
            if !row.continues {
                emit.end_line(newline);
            }
            lines_since_cancel_check += 1;
            bytes_since_cancel_check += body.len() + 1;
        }

        first_chunk = false;

        // Termination: the chunk says whether anything follows. ❗ ❌ Never "fewer rows
        // than I asked for": `CHUNK_BUDGET_BYTES` makes a short chunk ordinary, and
        // reading one as EOF would silently truncate a copy or a save.
        if chunk.end == ChunkEnd::EndOfFile {
            break;
        }

        // Advance by byte offset for the next chunk.
        next_target = SeekTarget::ByteOffset(chunk_end_offset);
    }

    // For the Eof case (or a short file that ended before reaching an explicit end), the
    // newline owed to the very last line emitted is dropped rather than paid: half-open
    // semantics say "include the last line's full content but not a final implicit
    // newline boundary marker beyond it".
    emit.finish(/*keep_trailing_newline=*/ false)
}

#[cfg(test)]
#[path = "range_read_test.rs"]
mod tests;

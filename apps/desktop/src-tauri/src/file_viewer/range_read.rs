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

use std::sync::atomic::{AtomicBool, Ordering};

use serde::Deserialize;

use super::{ChunkEnd, FileViewerBackend, SeekTarget, ViewerError};

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
/// The `\n` that joins two lines is held back until the next line arrives, so the
/// range's final newline (which half-open semantics drop) never has to be taken back
/// out of a piece that already left for the sink.
struct ChunkedSink<'a, S: FnMut(&str) -> Result<(), ViewerError>> {
    buf: String,
    chunk_bytes: usize,
    sink: &'a mut S,
    newline_owed: bool,
}

impl<'a, S: FnMut(&str) -> Result<(), ViewerError>> ChunkedSink<'a, S> {
    fn new(sink: &'a mut S, chunk_bytes: usize) -> Self {
        Self {
            buf: String::new(),
            chunk_bytes,
            sink,
            newline_owed: false,
        }
    }

    /// Appends `text`, first paying any newline owed to the previous line.
    fn push(&mut self, text: &str) -> Result<(), ViewerError> {
        self.pay_newline();
        self.buf.push_str(text);
        if self.buf.len() >= self.chunk_bytes {
            self.flush()?;
        }
        Ok(())
    }

    /// Ends a line: the `\n` lands only once something follows it.
    fn end_line(&mut self) {
        self.newline_owed = true;
    }

    fn pay_newline(&mut self) {
        if self.newline_owed {
            self.buf.push('\n');
            self.newline_owed = false;
        }
    }

    /// Hands what's left to the sink. `keep_trailing_newline` pays a newline still
    /// owed, which only the "ran past the end line" exit wants.
    fn finish(mut self, keep_trailing_newline: bool) -> Result<(), ViewerError> {
        if keep_trailing_newline {
            self.pay_newline();
        }
        self.flush()
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
    let mut out = String::new();
    let mut sink = |piece: &str| {
        out.push_str(piece);
        Ok(())
    };
    read_range_streamed(backend, anchor, focus, cancel, STREAM_CHUNK_BYTES, &mut sink)?;
    Ok(out)
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
/// its own error, and nothing further is read.
///
/// Streaming: after the initial seek by line number, the function advances by **byte
/// offset** rather than line number. This is mandatory for the ByteSeek backend, which
/// only estimates line numbers (`SeekTarget::Row(N)` resolves to `N * 80` bytes); for
/// FullLoad and LineIndex backends, byte-offset seeking is equally well-supported and
/// gives a single code path.
pub fn read_range_streamed<S: FnMut(&str) -> Result<(), ViewerError>>(
    backend: &dyn FileViewerBackend,
    anchor: RangeEnd,
    focus: RangeEnd,
    cancel: &AtomicBool,
    chunk_bytes: usize,
    sink: &mut S,
) -> Result<(), ViewerError> {
    let (start, end) = if compare_ends(&anchor, &focus).is_le() {
        (anchor, focus)
    } else {
        (focus, anchor)
    };

    // Resolve start row + offset. `Eof` as the start is unusual but well-defined:
    // empty selection at end of file.
    let (start_row, start_offset_utf16) = match start {
        RangeEnd::Row { row, offset } => (row as usize, offset),
        RangeEnd::Eof => return Ok(()),
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
        let line = chunk.rows.first().map(|row| &row.text).ok_or(ViewerError::OutOfRange)?;
        let start_byte = clamp_utf16_offset_to_byte(line, start_offset_utf16);
        let end_byte = clamp_utf16_offset_to_byte(line, end_offset_utf16);
        let lo = start_byte.min(end_byte);
        let hi = start_byte.max(end_byte);
        emit.push(&line[lo..hi])?;
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

        for row in &chunk.rows {
            let line = &row.text;
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

            let text = if is_first_overall {
                // First line of the whole selection: take from start_offset to end of line.
                &line[clamp_utf16_offset_to_byte(line, start_offset_utf16)..]
            } else if !end_is_eof && line_number == end_row {
                // Last line of an explicit range: take from offset 0 up to end_offset, and
                // no trailing delimiter (the range is half-open). Exits the walk here, so
                // it never reaches the `end_line()` below.
                let end_byte = clamp_utf16_offset_to_byte(line, end_offset_utf16);
                emit.push(&line[..end_byte])?;
                return emit.finish(/*keep_trailing_newline=*/ false);
            } else {
                &line[..]
            };
            emit.push(text)?;
            // The one place the walk decides a row carries its delimiter, and the one
            // line that had to change for rows: ❗ a break Cmdr made is NOT a newline,
            // so joining a continuing row to the next would put a line break in the
            // clipboard and in save-as that the file never contained.
            if !row.continues {
                emit.end_line();
            }
            lines_since_cancel_check += 1;
            bytes_since_cancel_check += text.len() + 1;
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
mod tests {
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
        let mut sink = |piece: &str| {
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
}

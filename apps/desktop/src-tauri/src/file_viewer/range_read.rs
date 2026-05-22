//! Stitches a `(line, offset)` -> `(line, offset)` range read into a single UTF-8 string,
//! independent of which backend the session uses.
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

use super::{FileViewerBackend, SeekTarget, ViewerError};

/// One endpoint of a selection. Frontend uses `Line { line, offset }`; for the
/// "select all" path in ByteSeek-no-index mode (where `totalLines` is unknown),
/// it uses `Eof` so the backend can resolve the end without a fake line number.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RangeEnd {
    Line { line: u64, offset: u32 },
    Eof,
}

impl RangeEnd {
    /// True if this endpoint is `Eof`.
    fn is_eof(&self) -> bool {
        matches!(self, Self::Eof)
    }
}

/// Compares two endpoints under the assumption that `Eof` is greater than every
/// `Line { ... }`. Returns `std::cmp::Ordering`.
fn compare_ends(a: &RangeEnd, b: &RangeEnd) -> std::cmp::Ordering {
    use std::cmp::Ordering as O;
    match (a, b) {
        (RangeEnd::Eof, RangeEnd::Eof) => O::Equal,
        (RangeEnd::Eof, _) => O::Greater,
        (_, RangeEnd::Eof) => O::Less,
        (RangeEnd::Line { line: la, offset: oa }, RangeEnd::Line { line: lb, offset: ob }) => {
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

/// Reads the selected range from the given backend, returning a single UTF-8 string.
///
/// Endpoints are normalised internally; reversed input (focus before anchor) returns
/// the same result as the forward range.
///
/// Returns `ViewerError::Cancelled` if `cancel` is flipped during the read,
/// `ViewerError::OutOfRange` if the requested line is past the file's last line
/// (with the exception that `Eof` is always valid).
///
/// Streaming: after the initial seek by line number, the function advances by **byte
/// offset** rather than line number. This is mandatory for the ByteSeek backend, which
/// only estimates line numbers (`SeekTarget::Line(N)` resolves to `N * 80` bytes); for
/// FullLoad and LineIndex backends, byte-offset seeking is equally well-supported and
/// gives a single code path.
pub fn read_range(
    backend: &dyn FileViewerBackend,
    anchor: RangeEnd,
    focus: RangeEnd,
    cancel: &AtomicBool,
) -> Result<String, ViewerError> {
    let (start, end) = if compare_ends(&anchor, &focus).is_le() {
        (anchor, focus)
    } else {
        (focus, anchor)
    };

    // Resolve start line + offset. `Eof` as the start is unusual but well-defined:
    // empty selection at end of file.
    let (start_line, start_offset_utf16) = match start {
        RangeEnd::Line { line, offset } => (line as usize, offset),
        RangeEnd::Eof => return Ok(String::new()),
    };

    // Validate start_line against backend's total_lines if known.
    if let Some(total) = backend.total_lines()
        && start_line >= total
    {
        return Err(ViewerError::OutOfRange);
    }

    // Resolve end. `Eof` means "to the last line, all of it"; otherwise we have an
    // explicit `Line { line, offset }`.
    let end_is_eof = end.is_eof();
    let (end_line, end_offset_utf16) = match end {
        RangeEnd::Line { line, offset } => (line as usize, offset),
        RangeEnd::Eof => (usize::MAX, 0),
    };

    let mut out = String::new();

    if start_line == end_line && !end_is_eof {
        // Single-line read: fetch the one line, clamp both offsets, slice between them.
        let chunk = backend.get_lines(&SeekTarget::Line(start_line), 1)?;
        let line = chunk.lines.first().ok_or(ViewerError::OutOfRange)?;
        let start_byte = clamp_utf16_offset_to_byte(line, start_offset_utf16);
        let end_byte = clamp_utf16_offset_to_byte(line, end_offset_utf16);
        let lo = start_byte.min(end_byte);
        let hi = start_byte.max(end_byte);
        out.push_str(&line[lo..hi]);
        if cancel.load(Ordering::Relaxed) {
            return Err(ViewerError::Cancelled);
        }
        return Ok(out);
    }

    // Multi-line streaming read. First chunk is keyed by start line (only call that
    // uses `SeekTarget::Line` so we land on the right starting line). Subsequent chunks
    // are keyed by **byte offset** of the end of the last chunk, which is exact for all
    // three backends (ByteSeek's `Line(N)` is approximate; its byte-offset seeks are
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
    let mut next_target = SeekTarget::Line(start_line);
    let mut lines_emitted: usize = 0;
    let mut first_chunk = true;
    let mut lines_since_cancel_check: usize = 0;
    let mut bytes_since_cancel_check: usize = 0;

    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(ViewerError::Cancelled);
        }

        let chunk = backend.get_lines(&next_target, FETCH_CHUNK)?;
        if chunk.lines.is_empty() {
            break;
        }

        // Compute the byte offset just past this chunk's last line.
        //
        // CRLF assumption: line readers in all three backends keep the `\r` AS PART of
        // the line string (they only split on `\n`; `&data[pos..pos + nl_pos]` retains
        // bytes before the newline byte). So `line.len()` already includes the `\r`
        // for CRLF files, and the `+ 1` accounts for the single `\n` delimiter byte.
        // No drift on either LF or CRLF files. See `byte_seek.rs:118`,
        // `full_load.rs:43`, `line_index.rs:172` for the parallel patterns. Test
        // fixture: `read_range_full_load_crlf_*` in `session_test.rs`.
        let mut chunk_end_offset = chunk.byte_offset;
        for line in &chunk.lines {
            chunk_end_offset += line.len() as u64 + 1;
        }

        let first_line_idx_in_chunk = chunk.first_line_number;

        for (i, line) in chunk.lines.iter().enumerate() {
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

            let line_number = first_line_idx_in_chunk + i;
            let is_first_overall = first_chunk && i == 0;

            // For explicit-end ranges, stop past the end line.
            if !end_is_eof && line_number > end_line {
                return Ok(out);
            }

            let bytes_before = out.len();
            if is_first_overall {
                // First line of the whole selection: take from start_offset to end of line.
                let start_byte = clamp_utf16_offset_to_byte(line, start_offset_utf16);
                out.push_str(&line[start_byte..]);
                out.push('\n');
            } else if !end_is_eof && line_number == end_line {
                // Last line of explicit range: take from offset 0 up to end_offset.
                let end_byte = clamp_utf16_offset_to_byte(line, end_offset_utf16);
                out.push_str(&line[..end_byte]);
                // No trailing newline on the end line of a half-open range.
                return Ok(out);
            } else {
                out.push_str(line);
                out.push('\n');
            }
            lines_emitted += 1;
            lines_since_cancel_check += 1;
            bytes_since_cancel_check += out.len() - bytes_before;
        }

        first_chunk = false;

        // Termination: backend returned fewer lines than requested means EOF.
        if chunk.lines.len() < FETCH_CHUNK {
            break;
        }

        // Advance by byte offset for the next chunk.
        next_target = SeekTarget::ByteOffset(chunk_end_offset);
    }

    // For the Eof case (or a short file that ended before reaching an explicit end),
    // trim the trailing newline we added for the very last line emitted. The half-open
    // semantics say "include the last line's full content but not a final implicit
    // newline boundary marker beyond it".
    if lines_emitted > 0
        && let Some(b'\n') = out.as_bytes().last().copied()
    {
        out.pop();
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn compare_ends_orders_eof_greatest() {
        let a = RangeEnd::Line { line: 5, offset: 3 };
        let b = RangeEnd::Line { line: 5, offset: 7 };
        let c = RangeEnd::Eof;
        assert!(compare_ends(&a, &b).is_lt());
        assert!(compare_ends(&b, &a).is_gt());
        assert!(compare_ends(&a, &c).is_lt());
        assert!(compare_ends(&c, &c).is_eq());
    }
}

//! The forward walk over the row boundary set, and what a fetch hands back.
//!
//! [`super::rows`] states the boundary rule and answers it in either direction from
//! one bounded window. This file walks it FORWARD, which is what every backend
//! actually does: [`RowReader`] yields row after row from a streaming buffer,
//! [`collect_rows`] turns that into one [`CollectedRows`] answer under the chunk
//! budget, and [`search_rows`] scans the same rows for matches.
//!
//! ❗ The rule lives in ONE place. Nothing here restates it; where a decision depends
//! on a clause, it names the clause and points at `rows.rs`. Two copies of a boundary
//! definition is how the two ends of a row stop agreeing, which is invariant I4.
//!
//! ❗ Nothing here may grow with a physical line's length (invariant I2). The buffer is
//! one refill chunk plus at most two segments, whatever the file.
//!
//! Architecture and the invariants (I1-I6): `DETAILS.md` § "Rows, not lines".

use std::collections::VecDeque;

use serde::Serialize;

use super::ViewerError;
use super::encoding::{FileEncoding, NewlineScanner, decode_line};
use super::rows::{RowRuler, RowSource, SEGMENT_BYTES, snap_back_to_char_start};

/// Where a file's content starts: past its BOM, if it actually has one.
///
/// ❗ `encoding.bom_bytes().len()` is NOT the answer. An encoding that CAN carry a BOM
/// doesn't mean this file does: `encoding::detect_from_head` reaches UTF-16 without one
/// through its parity heuristic, and a manual encoding switch lands a backend here too.
/// Assuming the BOM drops the file's first character from every read, and shifts one
/// backend's row 0 against another's, which the ByteSeek→LineIndex upgrade then slides
/// under a live row cache.
///
/// `head` is the file's first bytes; anything at least as long as the BOM will do.
pub fn content_start(head: &[u8], encoding: FileEncoding) -> u64 {
    let bom = encoding.bom_bytes();
    if !bom.is_empty() && head.starts_with(bom) {
        bom.len() as u64
    } else {
        0
    }
}

// ---------------------------------------------------------------------------
// Walking the boundary set forward
// ---------------------------------------------------------------------------

/// Where the next row boundary came from.
///
/// The label is what a row needs beyond the number: it decides whether the row ending
/// there `continues` (a break Cmdr made, which no copy path may turn into a newline)
/// and whether that row's text drops a trailing newline code unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextBoundary {
    /// Clause 2: the byte just past a newline. The row it closes ended at the file's
    /// own break, so its text stops one code unit short of it.
    LineStart(u64),
    /// Clause 3: a segment multiple, already snapped to a character start. The row it
    /// closes is one Cmdr ended itself, and keeps every byte as text.
    Segment(u64),
    /// EOF closes the last row.
    Eof(u64),
}

impl NextBoundary {
    pub fn offset(self) -> u64 {
        match self {
            Self::LineStart(b) | Self::Segment(b) | Self::Eof(b) => b,
        }
    }
}

/// The least row boundary greater than `boundary`, read forward.
///
/// ❗ The same boundary set as [`RowRuler`] approached from the other side, ❌ not a
/// second rule. `row_start` / `row_end` answer one probe with one bounded window,
/// which is what a seek wants and costs a two-segment read per row; a fetch walks
/// thousands of rows in sequence and reads the same boundaries out of the newline
/// stream it is already holding. `rows_test` asserts the two agree on every fixture at
/// every offset, so there is still only one definition to be wrong about.
///
/// `prev_newline` is the last newline code-unit START strictly below `boundary`,
/// `next_newline` the first at or above it. A caller may pass `None` for
/// `prev_newline` whenever it knows the segment below `boundary` holds no newline,
/// which is exactly what a clause-3 boundary proves.
///
/// `snap` applies clause 3's character snap, because the caller is the one holding the
/// bytes. A snapped multiple landing at or below `boundary` is discarded rather than
/// returned as a zero-length row, the same rejection `RowRuler::row_end` makes.
pub fn next_row_boundary<F: Fn(u64) -> u64>(
    boundary: u64,
    prev_newline: Option<u64>,
    next_newline: Option<u64>,
    newline_len: u64,
    segment: u64,
    total_bytes: u64,
    snap: F,
) -> NextBoundary {
    let mut best = NextBoundary::Eof(total_bytes);
    // Clause 3. Only two multiples can matter: the first above `boundary`, and the one
    // after it for when a newline inside the first one's segment disqualifies it.
    let first = boundary - boundary % segment + segment;
    for multiple in [first, first + segment] {
        if multiple > total_bytes || multiple >= best.offset() {
            break;
        }
        let evidence_start = multiple - segment;
        let clear =
            prev_newline.is_none_or(|unit| unit < evidence_start) && next_newline.is_none_or(|unit| unit >= multiple);
        if !clear {
            continue;
        }
        let snapped = snap(multiple);
        if snapped > boundary {
            best = NextBoundary::Segment(snapped);
            break;
        }
    }
    // Clause 2, which takes a tie: a boundary that is both a line start and a multiple
    // is the file's break, so the row ending there carries no marker.
    if let Some(unit) = next_newline {
        let line_start = unit + newline_len;
        if line_start > boundary && line_start <= best.offset() {
            best = NextBoundary::LineStart(line_start);
        }
    }
    best
}

/// One row's place in the file, before anything is decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowSpan {
    /// Absolute offset of the row's first byte.
    pub start: u64,
    /// Absolute offset of the next row's first byte: past the newline this row ended
    /// at, when it ended at one.
    pub end: u64,
    /// Absolute offset just past the row's last byte of TEXT: `end` less the newline
    /// code unit when the row ended at one, and `end` otherwise.
    pub text_end: u64,
    /// Cmdr ended this row at a segment boundary rather than at a newline or EOF.
    /// ❗ Nothing may join such a row to the next one with a newline.
    pub continues: bool,
    /// This row starts a physical line, so it is the one that prints a line number.
    pub starts_line: bool,
}

impl RowSpan {
    /// Bytes of text this row carries, newline excluded.
    pub fn text_bytes(&self) -> u64 {
        self.text_end - self.start
    }
}

/// How many bytes a [`RowReader`] pulls per refill. Big enough that a file of short
/// rows refills rarely; the compaction that precedes each refill moves every byte at
/// most once, so the walk stays linear whatever the row length.
const READ_CHUNK_BYTES: usize = 64 * 1024;

/// A forward walk over a file's rows.
///
/// Holds one bounded buffer (a refill chunk plus at most two segments), so walking a
/// 50 GB single-line file costs what walking a 50 KB one costs. ❗ Nothing here may
/// grow with a physical line's length: that is invariant I2, and a `memchr` "just to
/// find where this line ends" is how it gets lost.
pub struct RowReader<S: RowSource> {
    ruler: RowRuler<S>,
    encoding: FileEncoding,
    segment: u64,
    total_bytes: u64,
    /// The file's first content byte, past any BOM.
    ///
    /// The row GRID is absolute (`DETAILS.md` § "Rows, not lines": a BOM must not shift
    /// one backend's rows against another's), but a file's first row starts where its
    /// text starts. That is what makes all three backends agree on row 0, and what
    /// stops ByteSeek handing the user a selectable `U+FEFF`.
    content_start: u64,
    buf: Vec<u8>,
    /// Absolute offset of `buf[0]`.
    buf_start: u64,
    /// Newline code-unit starts inside `buf`, ascending, those below the cursor
    /// already dropped.
    newlines: VecDeque<u64>,
    scanner: NewlineScanner,
    /// Absolute offset of the next row's first byte.
    cursor: u64,
    /// The last newline code unit strictly below `cursor`, where knowing it can change
    /// an answer. `None` also covers "the segment below the cursor holds no newline",
    /// which is what a clause-3 boundary proves.
    prev_newline: Option<u64>,
    /// The row at `cursor` starts a physical line.
    at_line_start: bool,
    /// `buf` reaches EOF; no refill can add to it.
    filled_to_eof: bool,
    /// The walk has produced the file's last row.
    finished: bool,
    /// Where in `buf` the row [`RowReader::next_span`] last returned starts, so its
    /// text can still be decoded until the following call.
    last_row_at: usize,
}

impl<S: RowSource> RowReader<S> {
    pub fn new(source: S, encoding: FileEncoding, content_start: u64) -> Self {
        Self::with_ruler(RowRuler::new(source, encoding), encoding, SEGMENT_BYTES, content_start)
    }

    /// The same walk on a smaller grid, for tests only. See [`RowRuler::with_segment`].
    #[cfg(test)]
    pub fn with_segment(source: S, encoding: FileEncoding, content_start: u64, segment: u64) -> Self {
        Self::with_ruler(
            RowRuler::with_segment(source, encoding, segment),
            encoding,
            segment,
            content_start,
        )
    }

    fn with_ruler(ruler: RowRuler<S>, encoding: FileEncoding, segment: u64, content_start: u64) -> Self {
        let total_bytes = ruler.total_bytes();
        let content_start = content_start.min(total_bytes);
        Self {
            ruler,
            encoding,
            segment,
            total_bytes,
            content_start,
            buf: Vec::new(),
            buf_start: content_start,
            newlines: VecDeque::new(),
            scanner: NewlineScanner::new(encoding, content_start),
            cursor: content_start,
            prev_newline: None,
            at_line_start: true,
            filled_to_eof: false,
            finished: false,
            last_row_at: 0,
        }
    }

    /// Whether [`RowReader::next_span`] would return `None`: the walk has no row left.
    ///
    /// ❗ ❌ Not `cursor == total_bytes`. On a file ending in a newline that is true one
    /// row BEFORE the end, because the final empty row starts at that same offset. A
    /// caller using the offset as its tell either stops a row early (losing the file's
    /// last newline) or, having served that row, asks once more and is handed it again.
    pub fn at_end(&self) -> bool {
        self.finished || (self.cursor >= self.total_bytes && !self.at_line_start)
    }

    /// Absolute offset of the next row's first byte.
    pub fn cursor(&self) -> u64 {
        self.cursor
    }

    /// How far past a newline its line starts. 2 for UTF-16: the newline is a two-byte
    /// code unit. Stating the rule in code units is what keeps a UTF-16 row from
    /// starting mid-unit; it isn't an optimization to fold away.
    fn newline_len(&self) -> u64 {
        match self.encoding {
            FileEncoding::Utf16Le | FileEncoding::Utf16Be => 2,
            _ => 1,
        }
    }

    /// Put the walk on the row containing `offset`, and return that row's start.
    ///
    /// One bounded [`RowRuler::row_start_detail`] window, whatever the file's size or
    /// its longest line: this is the seek half of invariant I1.
    pub fn seek(&mut self, offset: u64) -> Result<u64, ViewerError> {
        let probe = offset.clamp(self.content_start, self.total_bytes);
        let found = self.ruler.row_start_detail(probe)?;
        let start = found.offset.max(self.content_start);
        // A clause-3 boundary has a proven newline-free segment behind it, and the
        // file's first row has nothing behind it at all; in both cases no newline below
        // the cursor can change a later answer.
        let prev_newline = if found.from_newline && start > self.content_start {
            Some(start - self.newline_len())
        } else {
            None
        };
        let starts_line = found.from_newline || start == self.content_start;
        self.reposition(start, starts_line, prev_newline);
        Ok(start)
    }

    fn reposition(&mut self, start: u64, starts_line: bool, prev_newline: Option<u64>) {
        self.buf.clear();
        self.buf_start = start;
        self.newlines.clear();
        self.scanner = NewlineScanner::new(self.encoding, start);
        self.cursor = start;
        self.prev_newline = prev_newline;
        self.at_line_start = starts_line;
        self.filled_to_eof = false;
        self.finished = false;
        self.last_row_at = 0;
    }

    /// The next row, or `None` once the file's last one has been produced.
    ///
    /// The row's bytes stay in the buffer until the NEXT call, which is exactly how
    /// long [`RowReader::last_text`] stays valid.
    pub fn next_span(&mut self) -> Result<Option<RowSpan>, ViewerError> {
        if self.finished {
            return Ok(None);
        }
        let start = self.cursor;
        if start >= self.total_bytes {
            self.finished = true;
            // A file that ends with a newline has one more row after it: the empty one
            // the cursor is sitting on. An empty file has exactly that row and nothing
            // else. Both match what `FullLoadBackend` has always produced, and taking
            // that answer for all three backends is what makes a whole-file copy carry
            // the file's final newline whatever the file's size.
            if self.at_line_start {
                self.last_row_at = self.buf.len();
                return Ok(Some(RowSpan {
                    start,
                    end: start,
                    text_end: start,
                    continues: false,
                    starts_line: true,
                }));
            }
            return Ok(None);
        }

        // Two segments past the cursor is every byte any clause can need: the furthest
        // a boundary can land is `cursor + 2 × segment`.
        self.fill_to(start + 2 * self.segment)?;
        while self.newlines.front().is_some_and(|unit| *unit < start) {
            self.newlines.pop_front();
        }
        let next_newline = self.newlines.front().copied();

        let (buf, buf_start, encoding) = (&self.buf, self.buf_start, self.encoding);
        let boundary = next_row_boundary(
            start,
            self.prev_newline,
            next_newline,
            self.newline_len(),
            self.segment,
            self.total_bytes,
            |multiple| snap_back_to_char_start(buf, buf_start, multiple, encoding),
        );

        let end = boundary.offset();
        let (text_end, continues) = match boundary {
            NextBoundary::LineStart(b) => (b - self.newline_len(), false),
            NextBoundary::Segment(b) => (b, true),
            NextBoundary::Eof(b) => (b, false),
        };
        let span = RowSpan {
            start,
            end,
            text_end,
            continues,
            starts_line: self.at_line_start,
        };

        self.last_row_at = (start - self.buf_start) as usize;
        self.cursor = end;
        self.at_line_start = matches!(boundary, NextBoundary::LineStart(_));
        self.prev_newline = match boundary {
            NextBoundary::LineStart(b) => Some(b - self.newline_len()),
            _ => None,
        };
        Ok(Some(span))
    }

    /// Decoded text of the row [`RowReader::next_span`] last returned. Valid until the
    /// next call to it.
    pub fn last_text(&self, span: &RowSpan) -> String {
        let hi = (self.last_row_at + span.text_bytes() as usize).min(self.buf.len());
        decode_line(&self.buf[self.last_row_at.min(hi)..hi], self.encoding)
    }

    /// The next row with its text: what a fetch and a search both want.
    pub fn next_row(&mut self) -> Result<Option<(RowSpan, String)>, ViewerError> {
        let Some(span) = self.next_span()? else {
            return Ok(None);
        };
        let text = self.last_text(&span);
        Ok(Some((span, text)))
    }

    /// Make sure the buffer reaches `want` (or EOF), dropping what the walk has passed.
    ///
    /// Compaction happens only here, with the cursor on a row start, so a row the
    /// caller still holds is never moved out from under it.
    fn fill_to(&mut self, want: u64) -> Result<(), ViewerError> {
        let target = want.min(self.total_bytes);
        while !self.filled_to_eof && self.buf_start + self.buf.len() as u64 <= target {
            let passed = (self.cursor - self.buf_start) as usize;
            if passed > 0 {
                self.buf.drain(..passed);
                self.buf_start += passed as u64;
            }
            let read_at = self.buf_start + self.buf.len() as u64;
            let filled = self.buf.len();
            self.buf.resize(filled + READ_CHUNK_BYTES, 0);
            let got = self.ruler.read_into(read_at, &mut self.buf[filled..])?;
            self.buf.truncate(filled + got);
            if got == 0 {
                self.filled_to_eof = true;
                break;
            }
            let big_endian = matches!(self.encoding, FileEncoding::Utf16Be);
            let sink = &mut self.newlines;
            self.scanner.feed(&self.buf[filled..], |offset| {
                // `NewlineScanner` reports the `0x0A` BYTE; UTF-16 BE holds it in the
                // second half of the pair, and the rule counts unit starts.
                sink.push_back(if big_endian { offset - 1 } else { offset });
            });
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// What a fetch hands back
// ---------------------------------------------------------------------------

/// The most text one `viewer_get_lines` answer may carry (2 MiB).
///
/// A fetch that hits it returns FEWER ROWS, never a shortened one: invariant I3 says
/// nothing is silently truncated. It keeps a wrap-on viewport from handing the
/// offscreen height measurer megabytes of text, and it is why a chunk has to SAY it
/// stopped early ([`ChunkEnd`]) rather than leave a caller guessing from the row count.
pub const CHUNK_BUDGET_BYTES: u64 = 2 * 1024 * 1024;

/// One row, as a backend serves it.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ViewerRow {
    pub text: String,
    /// Absolute offset of the row's first byte.
    pub byte_offset: u64,
    /// Cmdr ended this row at a segment boundary, not at a newline the file contains.
    /// The frontend marks it; ❗ ❌ no copy, save, or search path may join it to the
    /// next row with a newline.
    pub continues: bool,
    /// The 0-based physical line this row starts, or `None` on a continuation row (the
    /// gutter prints nothing there, the usual editor convention). An estimate on
    /// `ByteSeekBackend`, which has no line index; exact on the other two.
    pub line_number: Option<usize>,
}

/// Why a chunk holds the rows it holds.
///
/// ❗ A caller walking a file chunk by chunk steers by THIS, ❌ never by "fewer rows
/// than I asked for": [`CHUNK_BUDGET_BYTES`] makes a short chunk ordinary, and reading
/// one as EOF silently truncates a copy or a save.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ChunkEnd {
    /// Every row asked for was served. More may follow at `end_byte_offset`.
    CountReached,
    /// The chunk reached [`CHUNK_BUDGET_BYTES`] first. More follow at
    /// `end_byte_offset`; ask again from there.
    BudgetReached,
    /// The chunk reached the end of the file. There is nothing past it.
    EndOfFile,
}

/// How many rows a file has, and whether that is a count or an estimate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", content = "rows", rename_all = "camelCase")]
pub enum TotalRows {
    /// Counted: `FullLoadBackend` holds the file, or `LineIndexBackend` scanned it.
    Exact(usize),
    /// Derived from the file's size. `ByteSeekBackend` opens without a scan, so it
    /// divides by the bytes-per-row it sampled at open. That sample makes the number
    /// EXACT on a file with no newline in it (every row is a whole segment) and an
    /// estimate on anything else, exactly as its line count was an estimate before.
    Estimated(usize),
}

impl TotalRows {
    pub fn rows(self) -> usize {
        match self {
            Self::Exact(n) | Self::Estimated(n) => n,
        }
    }

    pub fn is_exact(self) -> bool {
        matches!(self, Self::Exact(_))
    }
}

/// What [`collect_rows`] produced.
pub struct CollectedRows {
    pub rows: Vec<ViewerRow>,
    pub end: ChunkEnd,
    /// Absolute offset just past the last row served. A caller asks for its next chunk
    /// from here; ❗ ❌ never from decoded string lengths, which are UTF-8 even when the
    /// file is not.
    pub end_byte_offset: u64,
}

/// Walk `reader` from wherever it sits, taking at most `count` rows and at most
/// [`CHUNK_BUDGET_BYTES`] of text.
///
/// `first_line` numbers the first row that starts a line; every line-starting row after
/// it takes the next number. `None` means the backend has no line numbers to give.
///
/// `cancel` is checked before each row; flipped, the walk returns `Cancelled`.
pub fn collect_rows<S: RowSource>(
    reader: &mut RowReader<S>,
    first_line: Option<usize>,
    count: usize,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<CollectedRows, ViewerError> {
    let mut rows = Vec::new();
    let mut taken = 0u64;
    let mut end = ChunkEnd::CountReached;
    let mut end_byte_offset = reader.cursor();
    let mut next_line = first_line;

    while rows.len() < count {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(ViewerError::Cancelled);
        }
        let Some((span, text)) = reader.next_row()? else {
            end = ChunkEnd::EndOfFile;
            break;
        };
        let line_number = if span.starts_line {
            let n = next_line;
            next_line = next_line.map(|n| n + 1);
            n
        } else {
            None
        };
        rows.push(ViewerRow {
            text,
            byte_offset: span.start,
            continues: span.continues,
            line_number,
        });
        taken += span.text_bytes();
        end_byte_offset = span.end;
        // Tested after the row lands, so a row is never cut in half to fit; a chunk
        // overshoots the budget by at most one row (under 40 KB) instead.
        if taken >= CHUNK_BUDGET_BYTES && rows.len() < count {
            end = ChunkEnd::BudgetReached;
            break;
        }
    }
    // ❗ A chunk that ran out of ROWS at the same moment it ran out of FILE is still at
    // the end, and has to say so. Reporting `CountReached` there sends the caller back
    // for one more chunk from `end_byte_offset`, and every streaming seek clamps that
    // onto the last row, so a copy or a save carries it twice (invariant I3).
    if reader.at_end() {
        end = ChunkEnd::EndOfFile;
    }
    Ok(CollectedRows {
        rows,
        end,
        end_byte_offset,
    })
}

/// Scan every row of a file with `matcher`, reporting matches by row and by column
/// within that row.
///
/// ❗ One implementation for both streaming backends. They used to carry a copy each of
/// the same `memchr(b'\n')` loop, which rebuilt `leftover + chunk` on every newline-free
/// chunk (about 1.4 TB of `memcpy` on a 300 MB line) and framed UTF-16 as if it were
/// ASCII, so ⌘F in a UTF-16 file found nothing at all. The walk is linear,
/// encoding-aware, and bounds a match's column by the row holding it.
///
/// Cancellation is checked per row and, inside `scan_line_with_matcher`, per match.
pub fn search_rows<S: RowSource>(
    reader: &mut RowReader<S>,
    matcher: &super::Matcher,
    cancel: &std::sync::atomic::AtomicBool,
    results: &std::sync::Mutex<Vec<super::SearchMatch>>,
    progress: &std::sync::Mutex<u64>,
) -> Result<u64, ViewerError> {
    use std::sync::atomic::Ordering;

    use crate::ignore_poison::IgnorePoison;

    use super::search_matcher::{LineScan, scan_line_with_matcher};

    let mut row_number = 0usize;
    let mut scanned = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Some((span, text)) = reader.next_row()? else { break };
        match scan_line_with_matcher(matcher, &text, row_number, span.start, cancel, results) {
            LineScan::HitLimit | LineScan::Cancelled => {
                scanned = span.end;
                break;
            }
            LineScan::Done => {}
        }
        scanned = span.end;
        row_number += 1;
        // Progress per row would lock 15 000 times on the reported file; once a segment
        // is often enough for a progress bar and cheap enough to ignore.
        if row_number.is_multiple_of(64) {
            *progress.lock_ignore_poison() = scanned;
        }
    }
    *progress.lock_ignore_poison() = scanned;
    Ok(scanned)
}

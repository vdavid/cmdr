//! The row boundary rule: where the viewer breaks a file into rows.
//!
//! The viewer renders **rows**, not physical lines. A row ends at a newline or
//! after a segment, whichever comes first, so a file with no newline in it costs
//! a bounded read per fetch instead of the whole file. Nothing is dropped; a long
//! line simply occupies several rows.
//!
//! The rule is defined once, as a **boundary set**, and both `row_start` and
//! `row_end` are derived from it. A byte offset `b` is a row boundary exactly
//! when one of these holds:
//!
//! 1. `b == 0`, or
//! 2. `b` is the start of a physical line (the offset just past a newline), or
//! 3. `b` is a multiple of [`SEGMENT_BYTES`] **below EOF** **and the window
//!    `[b - SEGMENT_BYTES, b)` contains no newline**, snapped back to the nearest
//!    character start.
//!
//! Then `row_start(offset)` is the greatest boundary `<= offset`, and
//! `row_end(row_start)` is the least boundary `> row_start`, or EOF.
//!
//! ❗ **"Below EOF" in clause 3 is load-bearing.** A multiple sitting exactly on EOF
//! cannot start a row, because nothing follows it. Admitting it looks harmless and is,
//! until the file ALSO ends mid-character: the snap then drags it back below EOF and
//! invents a boundary, splitting a couple of truncated bytes off as their own row. The
//! same byte then belongs to two rows depending on which direction you read the rule
//! from, which is invariant I4.
//!
//! ❗ **Clause 3's second half is the whole design, not an optimization.** A
//! `SEGMENT_BYTES`-wide window with no newline implies a line at least that long.
//! Contrapositive: if every line is shorter than `SEGMENT_BYTES`, no multiple ever
//! qualifies, no row ever ends anywhere but at a newline, and rows are exactly
//! lines. Drop the window test and every ordinary file gains a spurious break
//! every 20 000 bytes, because a line straddling a multiple gets cut. (A line of
//! EXACTLY `SEGMENT_BYTES` is not shorter than one, so it does split: into a full
//! row plus a row holding only its newline.)
//!
//! ❌ **Do not derive `row_end` as `row_start + SEGMENT_BYTES`.** It reads like the
//! same thing and is not: after a newline at byte 100, iterating gives a row
//! `[101, 20101)` while probing at 20050 gives `row_start = 20000`, so the same
//! byte belongs to two different rows and a byte-offset seek re-renders overlapping
//! text. Both ends come from the boundary set, always.
//!
//! **Boundaries are absolute file offsets, BOM or no BOM.** `full_load.rs` scans
//! BOM-stripped indices; anchoring rows to those would put FullLoad's rows out of
//! step with ByteSeek's across a reload or a tail escalation, which swap backends
//! under a live frontend row cache.
//!
//! The forward walk over this boundary set, and the row types a fetch hands back,
//! live in `row_walk.rs`. This file is the rule; that one reads it.
//!
//! Architecture and the invariants (I1-I6): `DETAILS.md` § "Rows, not lines".

use super::ViewerError;
use super::encoding::{FileEncoding, NewlineScanner};
use super::source::SharedSource;

/// The grid row boundaries sit on, and the length of every row inside a long line
/// except its first.
///
/// A row is at most just under `2 × SEGMENT_BYTES`: a newline exactly on a multiple
/// disqualifies the next one, so the row runs to the one after. Chosen for a
/// readable horizontal scroll with wrap off, and deliberately not a round power of
/// two, so a stray 16384 / 65536 elsewhere is obviously unrelated.
pub const SEGMENT_BYTES: u64 = 20_000;

/// The most bytes a single `row_start` or `row_end` call may read.
///
/// One window of this size answers everything either function needs: every
/// candidate boundary near the probe, plus the newline evidence that decides
/// clause 3 for each of them. Nothing here depends on where the physical line
/// starts or ends, which is what makes a 50 GB single-line file cheap.
pub const MAX_WINDOW_BYTES: u64 = 2 * SEGMENT_BYTES;
/// A row boundary, and which clause put it there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowStart {
    pub offset: u64,
    /// The boundary is the byte just past a newline, so the row starting here starts a
    /// physical line and the row BEFORE it ended at a break the file contains.
    pub from_newline: bool,
}

/// A file the rule can read windows of.
///
/// The rule never opens a file itself, so a caller hands it an in-memory slice
/// ([`SliceSource`], for `FullLoadBackend` and tests) or an open file
/// ([`FileSource`]). Tests wrap a source to count bytes, which is what makes the
/// bounded-work property structurally checkable instead of a timing guess.
pub trait RowSource {
    /// The file's size in bytes. Row boundaries never exceed it.
    fn total_bytes(&self) -> u64;

    /// Fill `buf` with bytes starting at `start`, returning how many were filled.
    /// A short fill means EOF, or a file that shrank under us.
    fn read_window(&mut self, start: u64, buf: &mut [u8]) -> Result<usize, ViewerError>;
}

/// A [`RowSource`] over bytes already in memory.
pub struct SliceSource<'a> {
    bytes: &'a [u8],
}

impl<'a> SliceSource<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }
}

impl RowSource for SliceSource<'_> {
    fn total_bytes(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_window(&mut self, start: u64, buf: &mut [u8]) -> Result<usize, ViewerError> {
        let start = (start as usize).min(self.bytes.len());
        let available = &self.bytes[start..];
        let filled = available.len().min(buf.len());
        buf[..filled].copy_from_slice(&available[..filled]);
        Ok(filled)
    }
}

/// A [`RowSource`] over a session's open file ([`SharedSource`]).
pub struct FileSource {
    source: SharedSource,
    total_bytes: u64,
}

impl FileSource {
    pub fn new(source: SharedSource, total_bytes: u64) -> Self {
        Self { source, total_bytes }
    }
}

impl RowSource for FileSource {
    fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// Never asks the source for bytes past `total_bytes`: the backend's size is the
    /// file as far as this walk knows it, and a volume source reads a request that
    /// runs past ITS size as "the file grew" (`volume_source.rs`).
    fn read_window(&mut self, start: u64, buf: &mut [u8]) -> Result<usize, ViewerError> {
        let wanted = self.total_bytes.saturating_sub(start).min(buf.len() as u64) as usize;
        if wanted == 0 {
            return Ok(0);
        }
        self.source.read_at(start, &mut buf[..wanted])
    }
}

/// Applies the row boundary rule to one source, reusing one window buffer.
///
/// Hold on to a ruler across a fetch: the buffer grows to [`MAX_WINDOW_BYTES`] and
/// refilling it beats reallocating it per row.
pub struct RowRuler<S: RowSource> {
    source: S,
    encoding: FileEncoding,
    /// The grid boundaries sit on. Always [`SEGMENT_BYTES`] outside tests, which
    /// use a tiny even segment so every property can be probed at every offset of
    /// dozens of crafted files instead of sampled on a handful of big ones.
    segment: u64,
    buf: Vec<u8>,
}

impl<S: RowSource> RowRuler<S> {
    pub fn new(source: S, encoding: FileEncoding) -> Self {
        Self {
            source,
            encoding,
            segment: SEGMENT_BYTES,
            buf: Vec::new(),
        }
    }

    /// The same rule on a smaller grid, for tests only.
    ///
    /// The segment must be even (UTF-16 code units are two bytes, and the grid has
    /// to stay code-unit aligned) and comfortably longer than the longest
    /// character, or clause 3's "a newline-free segment implies a long line"
    /// reasoning stops holding.
    #[cfg(test)]
    pub fn with_segment(source: S, encoding: FileEncoding, segment: u64) -> Self {
        assert!(
            segment >= 8 && segment.is_multiple_of(2),
            "segment must be even and >= 8"
        );
        Self {
            source,
            encoding,
            segment,
            buf: Vec::new(),
        }
    }

    pub fn total_bytes(&self) -> u64 {
        self.source.total_bytes()
    }

    /// The greatest row boundary `<= offset`.
    ///
    /// Reads exactly one window of at most [`MAX_WINDOW_BYTES`], whatever the
    /// offset and whatever the file size.
    ///
    /// Production seeks through [`RowRuler::row_start_detail`], which answers this plus
    /// the clause that placed the boundary. This plain form is the shape the property
    /// tests state the rule in, and is kept for them.
    #[cfg(test)]
    pub fn row_start(&mut self, offset: u64) -> Result<u64, ViewerError> {
        Ok(self.row_start_detail(offset)?.offset)
    }

    /// `RowRuler::row_start`, plus which clause put the boundary there.
    ///
    /// A forward walk needs both halves: `from_newline` is what makes a row print a
    /// line number in its gutter, and it is also the entire newline evidence the NEXT
    /// boundary needs (see [`super::row_walk::next_row_boundary`]). So a fetch pays for one window on
    /// its first row and none on the thousands after it.
    pub fn row_start_detail(&mut self, offset: u64) -> Result<RowStart, ViewerError> {
        let total = self.source.total_bytes();
        let offset = offset.min(total);
        let segment = self.segment;
        // The window spans the multiple at or below `offset` and the one above it.
        // That holds everything any clause can need: the newline evidence for both
        // candidate multiples, and every line start at or below `offset`.
        //
        // ❗ The grid is measured from the last BYTE, not from `offset`, which differs
        // only when `offset == total`. No boundary sits at EOF, so a probe there is
        // asking which row the file's last byte belongs to; measuring from `offset`
        // would centre the window one segment too high and find no candidate at all.
        let grid_probe = if offset >= total {
            total.saturating_sub(1)
        } else {
            offset
        };
        let grid = grid_probe - grid_probe % segment;
        let window_start = grid.saturating_sub(segment);
        let window_end = (grid + segment).min(total);
        self.load_window(window_start, window_end)?;
        let window = Window::new(&self.buf, window_start, self.encoding, segment, total);

        // Clause 1 is the floor: 0 is always a boundary.
        let mut best = 0u64;
        let mut from_newline = false;
        // Clause 3: the multiple at or below `offset`, plus the one above it,
        // which a character straddling it can pull back to at or below `offset`.
        for multiple in [grid, grid + segment] {
            if let Some(boundary) = window.clause_three_boundary(multiple)
                && boundary <= offset
                && boundary > best
            {
                best = boundary;
                from_newline = false;
            }
        }
        // Clause 2: the last line start at or below `offset`. It takes a tie with a
        // multiple, because a boundary that is both is one the FILE contains: the row
        // ending there is the file's break, not ours.
        if let Some(line_start) = window.last_line_start_at_or_below(offset)
            && line_start >= best
        {
            best = line_start;
            from_newline = true;
        }
        Ok(RowStart {
            offset: best,
            from_newline,
        })
    }

    /// The least row boundary `> row_start`, or EOF.
    ///
    /// `row_start` is expected to be a boundary, one `RowRuler::row_start`
    /// returned. Reads one window of at most [`MAX_WINDOW_BYTES`].
    ///
    /// ❗ This is the rule's canonical forward step, and the reference the streaming
    /// [`next_row_boundary`] is checked against on every fixture. Production reads
    /// forward instead, because a window per row would cost a 4 096-row fetch ~160 MB;
    /// ❌ don't delete this as unused, it is what keeps that walk honest.
    #[cfg(test)]
    pub fn row_end(&mut self, row_start: u64) -> Result<u64, ViewerError> {
        let total = self.source.total_bytes();
        if row_start >= total {
            return Ok(total);
        }
        let segment = self.segment;
        // The next boundary always lands below `row_start + 2 × segment`, so the
        // two multiples above `row_start` are the only clause-3 candidates. The
        // window opens a segment below the first of them, where its own newline
        // evidence begins.
        let next_multiple = row_start - row_start % segment + segment;
        let window_start = next_multiple - segment;
        let window_end = (next_multiple + segment).min(total);
        self.load_window(window_start, window_end)?;
        let window = Window::new(&self.buf, window_start, self.encoding, segment, total);

        // EOF ends the last row; anything below it wins.
        let mut best = total;
        if let Some(line_start) = window.first_line_start_above(row_start) {
            best = best.min(line_start);
        }
        for multiple in [next_multiple, next_multiple + segment] {
            if let Some(boundary) = window.clause_three_boundary(multiple)
                && boundary > row_start
            {
                best = best.min(boundary);
            }
        }
        Ok(best)
    }

    /// Read straight from the underlying source, for a caller that does its own
    /// buffering ([`super::row_walk::RowReader`]). The ruler's own window is untouched.
    pub(super) fn read_into(&mut self, start: u64, buf: &mut [u8]) -> Result<usize, ViewerError> {
        self.source.read_window(start, buf)
    }

    /// Refill `self.buf` with `[start, end)`. A short read means EOF, or a file
    /// that shrank under us; the rule then works from what it got.
    fn load_window(&mut self, start: u64, end: u64) -> Result<(), ViewerError> {
        let len = end.saturating_sub(start) as usize;
        debug_assert!(
            len as u64 <= 2 * self.segment && (self.segment != SEGMENT_BYTES || len as u64 <= MAX_WINDOW_BYTES),
            "a window may never exceed two segments"
        );
        self.buf.resize(len, 0);
        let filled = self.source.read_window(start, &mut self.buf[..len])?;
        self.buf.truncate(filled);
        Ok(())
    }
}

/// One loaded window, and the three clauses read out of it.
///
/// Every boundary question the rule asks is answerable from a window of two
/// segments, which is the whole reason a 50 GB single-line file is cheap: nothing
/// here needs to know where the physical line starts or ends.
struct Window<'a> {
    bytes: &'a [u8],
    /// Absolute offset of `bytes[0]`. Always a multiple of the segment, or 0, so
    /// the UTF-16 scan starts code-unit aligned.
    start: u64,
    /// Absolute offsets of the code units holding `U+000A`, ascending.
    newlines: Vec<u64>,
    encoding: FileEncoding,
    segment: u64,
    total_bytes: u64,
}

impl<'a> Window<'a> {
    fn new(bytes: &'a [u8], start: u64, encoding: FileEncoding, segment: u64, total_bytes: u64) -> Self {
        Self {
            bytes,
            start,
            newlines: newline_unit_starts(bytes, start, encoding),
            encoding,
            segment,
            total_bytes,
        }
    }

    /// One past the last byte actually read. Below the requested end when the file
    /// shrank under us.
    fn end(&self) -> u64 {
        self.start + self.bytes.len() as u64
    }

    /// How far past a newline its line starts, in bytes.
    ///
    /// ❗ 2 for UTF-16: the newline is a two-byte code unit, so the line starts at
    /// `unit + 2`. Stating the rule in code units rather than raw bytes is what
    /// keeps a UTF-16 row from starting mid-unit; it isn't an optimization to fold
    /// away.
    fn newline_len(&self) -> u64 {
        match self.encoding {
            FileEncoding::Utf16Le | FileEncoding::Utf16Be => 2,
            _ => 1,
        }
    }

    /// Clause 2, looking back: the greatest line start `<= limit`.
    fn last_line_start_at_or_below(&self, limit: u64) -> Option<u64> {
        self.newlines
            .iter()
            .rev()
            .map(|unit| unit + self.newline_len())
            .find(|line_start| *line_start <= limit)
    }

    /// Clause 2, looking forward: the least line start `> floor`.
    #[cfg(test)]
    fn first_line_start_above(&self, floor: u64) -> Option<u64> {
        self.newlines
            .iter()
            .map(|unit| unit + self.newline_len())
            .find(|line_start| *line_start > floor)
    }

    /// Clause 3: the boundary `multiple` contributes, if it contributes one.
    ///
    /// `None` when the multiple is 0 or past EOF (clause 1 covers the first, and a
    /// boundary past the last byte would end a row nobody can render), when a
    /// newline falls in the segment before it, or when the window came up short of
    /// the evidence needed to decide. Otherwise the multiple, snapped back to the
    /// nearest character start.
    ///
    /// ❗ The newline test is what keeps ordinary files whole. A newline-free
    /// segment can only sit inside a line at least that long, so in a file whose
    /// lines are all shorter than a segment NO multiple ever qualifies and rows
    /// come out exactly equal to lines. Drop the test and every ordinary file
    /// gains a spurious break every segment, because a line straddling a multiple
    /// gets cut.
    fn clause_three_boundary(&self, multiple: u64) -> Option<u64> {
        // ❗ `>=`, not `>`. A multiple sitting exactly ON EOF cannot start a row, because
        // nothing follows it. Admitting it is harmless until the file ALSO ends
        // mid-character: the snap then drags it back below EOF and invents a boundary,
        // splitting a couple of truncated bytes off as their own row. `row_start` would
        // put the file's last byte in that invented row while the forward walk put it in
        // the row a segment earlier, which is the same byte in two rows (I4).
        if multiple == 0 || multiple >= self.total_bytes {
            return None;
        }
        let evidence_start = multiple - self.segment;
        if evidence_start < self.start || multiple > self.end() {
            // The window doesn't hold the whole preceding segment, so we can't
            // prove it is newline-free. Treat the multiple as disqualified:
            // falling back to an earlier boundary is safe, inventing one is not.
            return None;
        }
        if self
            .newlines
            .iter()
            .any(|unit| *unit >= evidence_start && *unit < multiple)
        {
            return None;
        }
        Some(snap_back_to_char_start(self.bytes, self.start, multiple, self.encoding))
    }
}

/// The greatest character start `<= pos`, decided from the bytes below `pos` alone.
///
/// `bytes` is any window holding them; `bytes_start` is its absolute offset. Free
/// rather than a [`Window`] method because the forward walk ([`super::row_walk::RowReader`]) holds its
/// own buffer and has to snap clause 3's multiples with the same arithmetic. See
/// [`Window::clause_three_boundary`] for why only clause 3 needs this.
///
/// A boundary inside a character would make the decoder emit a replacement character
/// we invented ourselves, on both sides of the break. Reading only downward is what
/// lets a window ending exactly at `pos` still decide it, which is how the ruler fits
/// its reads in one segment pair. The snap moves at most three bytes, so it can't
/// reach the previous boundary (a qualifying multiple has a whole newline-free segment
/// behind it) and can't push a row past its length bound.
pub(super) fn snap_back_to_char_start(bytes: &[u8], bytes_start: u64, pos: u64, encoding: FileEncoding) -> u64 {
    if pos <= bytes_start {
        return pos;
    }
    let idx = (pos - bytes_start) as usize;
    if idx > bytes.len() {
        return pos;
    }
    match encoding {
        FileEncoding::Utf16Le | FileEncoding::Utf16Be => {
            if idx < 2 {
                return pos;
            }
            let unit = if matches!(encoding, FileEncoding::Utf16Le) {
                u16::from_le_bytes([bytes[idx - 2], bytes[idx - 1]])
            } else {
                u16::from_be_bytes([bytes[idx - 2], bytes[idx - 1]])
            };
            // A high surrogate right below `pos` means its low half starts AT
            // `pos`: one unit back keeps the pair in one row. Capped at one
            // step, because a RUN of lone high surrogates is malformed input
            // and an unbounded walk would break the row length bound.
            if (0xD800..=0xDBFF).contains(&unit) {
                pos - 2
            } else {
                pos
            }
        }
        FileEncoding::Utf8 | FileEncoding::Utf8WithBom | FileEncoding::UsAscii => {
            // Walk back over continuation bytes (at most three), then ask
            // whether the lead byte below them covers `pos`.
            let mut back = 0usize;
            while back < 3 && idx >= back + 2 && is_utf8_continuation(bytes[idx - back - 1]) {
                back += 1;
            }
            if idx < back + 1 {
                return pos;
            }
            let lead_pos = pos - back as u64 - 1;
            let char_len = utf8_char_len(bytes[idx - back - 1]);
            if lead_pos + char_len > pos { lead_pos } else { pos }
        }
        // The Western single-byte encodings map every byte to a character, so
        // no offset can split one.
        FileEncoding::Windows1252 | FileEncoding::Iso8859_1 | FileEncoding::MacRoman => pos,
    }
}

/// Byte offsets of the code unit holding `U+000A`, absolute, ascending.
///
/// [`NewlineScanner`] reports the offset of the `0x0A` BYTE, which for UTF-16 BE is
/// the second half of the pair; the rule wants the unit's own start, because that
/// is what the segment window is measured in and what `+ newline_len` turns into a
/// line start.
fn newline_unit_starts(window: &[u8], window_start: u64, encoding: FileEncoding) -> Vec<u64> {
    let mut scanner = NewlineScanner::new(encoding, window_start);
    let big_endian = matches!(encoding, FileEncoding::Utf16Be);
    let mut out = Vec::new();
    scanner.feed(window, |offset| {
        out.push(if big_endian { offset - 1 } else { offset });
    });
    out
}

fn is_utf8_continuation(byte: u8) -> bool {
    byte & 0xC0 == 0x80
}

/// Bytes in the character a lead byte starts. A malformed lead counts as 1, which
/// leaves the boundary where it was.
fn utf8_char_len(lead: u8) -> u64 {
    match lead {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1,
    }
}

//! ByteSeekBackend: byte-offset seeking with no pre-scan.
//!
//! Opens the file and can immediately serve rows at any byte position. The shared row
//! rule (`file_viewer::rows`) decides where a row starts and ends, so every read is
//! bounded: a 50 GB single-line file costs tens of kilobytes per fetch.
//!
//! Supports Fraction seeking by multiplying fraction × total_bytes. Row seeking goes
//! through `bytes_per_row`, a sample taken once at open: this backend has no index, so
//! its row numbers are estimates mid-file, exactly as its line numbers were.

use std::fs::File;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use log::debug;

use super::encoding::{FileEncoding, detect};
use super::row_walk::{self, RowReader, TotalRows, collect_rows, search_rows};
use super::rows::{FileSource, SEGMENT_BYTES};
use super::search_matcher::Matcher;
use super::{BackendCapabilities, FileViewerBackend, LineChunk, SearchMatch, SeekTarget, ViewerError};

/// How much of the file `sample_bytes_per_row` reads at open. Bounded, so opening stays
/// instant on any file; one refill chunk is plenty to see whether a file has newlines
/// and roughly how far apart they sit.
const BYTES_PER_ROW_SAMPLE: u64 = 64 * 1024;

pub struct ByteSeekBackend {
    path: std::path::PathBuf,
    total_bytes: u64,
    file_name: String,
    encoding: FileEncoding,
    /// Average source bytes per row, sampled once at open.
    ///
    /// ❗ This is the ONLY map between a row number and a byte offset this backend has,
    /// and it is used in both directions, so the two agree by construction. The old
    /// `n * 80` guess was used one way only, which is why selecting a range on a file
    /// over 1 MB and copying it came back empty.
    ///
    /// On a file with no newline in it the sample sees whole segments and the number
    /// comes out exactly `SEGMENT_BYTES`, making row numbers EXACT on precisely the
    /// file this backend exists for. On an ordinary file it is an estimate, as before.
    bytes_per_row: u64,
    /// The file's first content byte, past any BOM. Decided once at open: this backend
    /// opens the file per fetch and per search, and re-sniffing the head each time would
    /// buy nothing on an immutable value.
    content_start: u64,
}

/// The file's content start: past a BOM if it has one, through the one rule all three
/// backends share (`row_walk::content_start`), so row 0 is the same row in each of them.
fn detect_content_start(path: &Path, encoding: FileEncoding) -> u64 {
    let bom = encoding.bom_bytes();
    if bom.is_empty() {
        return 0;
    }
    let mut head = vec![0u8; bom.len()];
    match File::open(path).map(|mut f| std::io::Read::read(&mut f, &mut head)) {
        Ok(Ok(read)) => row_walk::content_start(&head[..read], encoding),
        _ => 0,
    }
}

impl ByteSeekBackend {
    /// Open with auto-detected encoding.
    pub fn open(path: &Path) -> Result<Self, ViewerError> {
        let encoding = detect(path).unwrap_or(FileEncoding::Utf8);
        Self::open_with_encoding(path, encoding)
    }

    pub fn open_with_encoding(path: &Path, encoding: FileEncoding) -> Result<Self, ViewerError> {
        let metadata = std::fs::metadata(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => ViewerError::NotFound {
                path: path.display().to_string(),
            },
            _ => ViewerError::from(e),
        })?;
        if metadata.is_dir() {
            return Err(ViewerError::IsDirectory);
        }

        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());

        let total_bytes = metadata.len();
        let content_start = detect_content_start(path, encoding);
        let bytes_per_row = sample_bytes_per_row(path, encoding, total_bytes, content_start)?;

        Ok(Self {
            path: path.to_path_buf(),
            total_bytes,
            file_name,
            encoding,
            bytes_per_row,
            content_start,
        })
    }

    /// Returns a fresh backend with `total_bytes = new_size`. The backend is
    /// immutable; tail-mode extension produces a new instance and `ArcSwap`s it
    /// into place. Cancellable for symmetry with `LineIndexBackend::extend_to`.
    ///
    /// The bytes-per-row sample carries over: re-sampling on every append would
    /// renumber rows under a live frontend cache for no gain.
    pub fn extend_to(&self, new_size: u64, _cancel: &AtomicBool) -> Self {
        Self {
            path: self.path.clone(),
            total_bytes: new_size,
            file_name: self.file_name.clone(),
            encoding: self.encoding,
            bytes_per_row: self.bytes_per_row,
            content_start: self.content_start,
        }
    }

    /// A row walk over the file, positioned at the row containing `offset`.
    fn reader_at(&self, offset: u64) -> Result<(RowReader<FileSource>, u64), ViewerError> {
        let file = File::open(&self.path)?;
        let mut reader = RowReader::new(
            FileSource::new(file, self.total_bytes),
            self.encoding,
            self.content_start,
        );
        let start = reader.seek(offset)?;
        Ok((reader, start))
    }

    /// The row index this backend calls the row at `offset`.
    fn row_at(&self, offset: u64) -> usize {
        (offset / self.bytes_per_row) as usize
    }

    /// The byte offset a target names, before the row rule snaps it to a boundary.
    fn resolve_byte_offset(&self, target: &SeekTarget) -> u64 {
        match target {
            SeekTarget::ByteOffset(offset) => (*offset).min(self.total_bytes),
            SeekTarget::Fraction(f) => {
                let f = f.clamp(0.0, 1.0);
                (f * self.total_bytes as f64) as u64
            }
            // No index, so a row number maps back through the same `bytes_per_row` that
            // produced it. ❗ ❌ Not `SEGMENT_BYTES`: rows equal lines on an ordinary
            // file, and a 20 000-byte stride would put row 10 past the end of a
            // 200-byte-a-row file, which is how the old 80-byte guess emptied a
            // clipboard. The sample collapses TO `SEGMENT_BYTES` by itself on a file
            // with no newline in it.
            SeekTarget::Row(row) => (*row as u64).saturating_mul(self.bytes_per_row).min(self.total_bytes),
        }
    }
}

/// Average source bytes per row over the first [`BYTES_PER_ROW_SAMPLE`] bytes.
///
/// Counts only rows that END inside the sample, so a partial last row can't drag the
/// average down. A file with no newline in it yields whole segments and therefore
/// exactly `SEGMENT_BYTES`; an empty or tiny file falls back to the segment size, which
/// keeps the divisor away from zero.
fn sample_bytes_per_row(
    path: &Path,
    encoding: FileEncoding,
    total_bytes: u64,
    content_start: u64,
) -> Result<u64, ViewerError> {
    let file = File::open(path)?;
    let mut reader = RowReader::new(FileSource::new(file, total_bytes), encoding, content_start);
    let limit = BYTES_PER_ROW_SAMPLE.min(total_bytes);
    let mut rows = 0u64;
    let mut consumed = content_start;
    while consumed < limit {
        let Some(span) = reader.next_span()? else { break };
        if span.end > limit || span.end == span.start {
            break;
        }
        rows += 1;
        consumed = span.end;
    }
    if rows == 0 || consumed <= content_start {
        return Ok(SEGMENT_BYTES);
    }
    Ok(((consumed - content_start) / rows).max(1))
}

impl FileViewerBackend for ByteSeekBackend {
    fn extend_to_boxed(&self, new_size: u64, cancel: &AtomicBool) -> Result<Box<dyn FileViewerBackend>, ViewerError> {
        Ok(Box::new(self.extend_to(new_size, cancel)))
    }

    fn with_encoding(&self, new_encoding: FileEncoding) -> Option<Box<dyn FileViewerBackend>> {
        if !super::encoding::same_byte_layout(self.encoding, new_encoding) {
            return None;
        }
        Some(Box::new(Self {
            path: self.path.clone(),
            total_bytes: self.total_bytes,
            file_name: self.file_name.clone(),
            encoding: new_encoding,
            bytes_per_row: self.bytes_per_row,
            content_start: self.content_start,
        }))
    }

    fn get_lines(&self, target: &SeekTarget, count: usize, cancel: &AtomicBool) -> Result<LineChunk, ViewerError> {
        let raw_offset = self.resolve_byte_offset(target);
        let (mut reader, row_start) = self.reader_at(raw_offset)?;

        // A row target keeps the number it asked for; anything else is placed on the
        // same grid the target would have produced. Both directions run through
        // `bytes_per_row`, so a row the frontend was handed comes back as itself.
        let first_row_number = match target {
            SeekTarget::Row(row) => *row,
            _ => self.row_at(row_start),
        };
        // No index, so the line number under a row is the row number: on an ordinary
        // file rows and lines are one-to-one, and inside a long line every continuation
        // row prints nothing at all. An estimate, like every number this backend gives.
        let collected = collect_rows(&mut reader, Some(first_row_number), count, cancel)?;

        debug!(
            "ByteSeekBackend::get_lines: target={:?} -> byte {}, row {} ({} rows, {:?})",
            target,
            row_start,
            first_row_number,
            collected.rows.len(),
            collected.end
        );

        Ok(LineChunk {
            rows: collected.rows,
            first_row_number,
            byte_offset: row_start,
            end_byte_offset: collected.end_byte_offset,
            end: collected.end,
            total_rows: self.total_rows(),
            total_bytes: self.total_bytes,
        })
    }

    /// Scan the file row by row.
    ///
    /// ❗ Rows, not `memchr(b'\n')` over raw bytes. That older loop rebuilt
    /// `leftover + chunk` on every newline-free chunk (about 1.4 TB of `memcpy` on a
    /// 300 MB line) and framed UTF-16 as if it were ASCII, so ⌘F in a UTF-16 file found
    /// nothing at all. The walk is linear, encoding-aware, and bounds each match's
    /// column by the row it sits in.
    fn search(
        &self,
        matcher: &Matcher,
        cancel: &AtomicBool,
        results: &Mutex<Vec<SearchMatch>>,
        progress: &Mutex<u64>,
    ) -> Result<u64, ViewerError> {
        let (mut reader, _) = self.reader_at(0)?;
        search_rows(&mut reader, matcher, cancel, results, progress)
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            supports_line_seek: false,
            supports_byte_seek: true,
            supports_fraction_seek: true,
            knows_total_lines: false,
        }
    }

    fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    fn total_rows(&self) -> TotalRows {
        // A count, not nothing: the frontend needs a scroll extent from the first
        // fetch, and `bytes_per_row` is the same map every row number here rides on.
        // Exact on a newline-free file; an estimate otherwise, which the type says.
        TotalRows::Estimated((self.total_bytes.div_ceil(self.bytes_per_row) as usize).max(1))
    }

    fn total_lines(&self) -> Option<usize> {
        None
    }

    fn file_name(&self) -> &str {
        &self.file_name
    }
}

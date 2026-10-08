//! LineIndexBackend: sparse ROW-offset index for efficient row-based seeking.
//!
//! Stores a byte offset every INDEX_CHECKPOINT_INTERVAL rows (256 by default), each
//! carrying the physical line number there as well, so the gutter gets exact numbers
//! out of the same single scan.
//!
//! ❗ Rows, not lines. A file with no newline in it has ONE line, so a line-counted
//! interval gave it a single checkpoint and every fetch rescanned from byte 0: the
//! backend broke the bounded-work invariant on precisely the file rows exist for. (And
//! it IS reached on such a file: the scan finishes well inside the indexing timeout.)
//!
//! The index is built by walking the file's rows once. After scanning, supports O(1)
//! row-based seeking via the checkpoint array.

use std::fs::File;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use super::encoding::FileEncoding;
use super::row_walk::{self, RowReader, TotalRows, collect_rows, search_rows};
use super::rows::FileSource;
use super::search_matcher::Matcher;
use super::{
    BackendCapabilities, FileViewerBackend, INDEX_CHECKPOINT_INTERVAL, LineChunk, SearchMatch, SeekTarget, ViewerError,
};

/// Test-only counter incremented every time `LineIndexBackend::open_with_encoding`
/// runs. Lets tests assert the instant-swap path actually skips the rebuild.
#[cfg(test)]
static OPEN_CALL_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
#[allow(dead_code, reason = "consumed by session_test instant-swap test")]
pub fn test_only_open_call_count() -> usize {
    OPEN_CALL_COUNT.load(Ordering::Relaxed)
}

/// Read enough of `path` to decide where its content starts, through the one rule all
/// three backends share (`row_walk::content_start`).
fn read_content_start(path: &Path, encoding: FileEncoding) -> Result<u64, ViewerError> {
    let bom = encoding.bom_bytes();
    if bom.is_empty() {
        return Ok(0);
    }
    let mut head = vec![0u8; bom.len()];
    let mut file = File::open(path)?;
    let read = std::io::Read::read(&mut file, &mut head)?;
    Ok(row_walk::content_start(&head[..read], encoding))
}

/// A checkpoint in the row index.
///
/// Carries BOTH coordinates because they answer different questions and a second pass
/// to recover either one would break the bounded-work invariant: `row` is what a seek
/// counts in, `line` is what the gutter prints.
#[derive(Debug, Clone)]
struct Checkpoint {
    row: usize,
    /// Physical lines that START at or before this row. The row at `row` prints
    /// `line` when it starts a line, and nothing when it continues one.
    line: usize,
    /// Absolute file offset of the FIRST byte of the row at index `row`.
    offset: u64,
}

pub struct LineIndexBackend {
    path: std::path::PathBuf,
    total_bytes: u64,
    file_name: String,
    /// Sparse index: one checkpoint every INDEX_CHECKPOINT_INTERVAL rows.
    checkpoints: Vec<Checkpoint>,
    /// Total rows discovered during the scan.
    total_rows: usize,
    /// Total physical lines discovered during the same scan.
    total_lines: usize,
    /// The file's first content byte, past any BOM.
    content_start: u64,
    encoding: FileEncoding,
}

impl LineIndexBackend {
    /// Build the line index by scanning the file. Auto-detects encoding.
    ///
    /// Test-only: production always opens through `open_with_encoding` with an
    /// explicit detected encoding (the session detects once and shares it).
    #[cfg(test)]
    pub fn open(path: &Path, cancel: &AtomicBool) -> Result<Self, ViewerError> {
        let encoding = super::encoding::detect(path).unwrap_or(FileEncoding::Utf8);
        Self::open_with_encoding(path, encoding, cancel)
    }

    pub fn open_with_encoding(path: &Path, encoding: FileEncoding, cancel: &AtomicBool) -> Result<Self, ViewerError> {
        #[cfg(test)]
        OPEN_CALL_COUNT.fetch_add(1, Ordering::Relaxed);
        let metadata = std::fs::metadata(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => ViewerError::NotFound {
                path: path.display().to_string(),
            },
            _ => ViewerError::from(e),
        })?;
        if metadata.is_dir() {
            return Err(ViewerError::IsDirectory);
        }

        let total_bytes = metadata.len();
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());

        // Walk the file's rows ONCE, recording both coordinates as we go. A second pass
        // to recover either one would read the file twice and break invariant I1.
        let content_start = read_content_start(path, encoding)?;

        let file = File::open(path)?;
        let mut reader = RowReader::new(FileSource::new(file, total_bytes), encoding, content_start);
        let mut checkpoints = Vec::new();
        let mut rows = 0usize;
        let mut lines = 0usize;
        while let Some(span) = reader.next_span()? {
            if rows.is_multiple_of(INDEX_CHECKPOINT_INTERVAL) {
                checkpoints.push(Checkpoint {
                    row: rows,
                    line: lines,
                    offset: span.start,
                });
            }
            if span.starts_line {
                lines += 1;
            }
            rows += 1;
            // Cancellation is checked per checkpoint interval rather than per row: on a
            // file of short rows a per-row atomic load would dominate the scan.
            if rows.is_multiple_of(INDEX_CHECKPOINT_INTERVAL) && cancel.load(Ordering::Relaxed) {
                return Err(ViewerError::Cancelled);
            }
        }

        Ok(Self {
            path: path.to_path_buf(),
            total_bytes,
            file_name,
            checkpoints,
            total_rows: rows.max(1),
            total_lines: lines.max(1),
            content_start,
            encoding,
        })
    }

    /// Returns a fresh backend with checkpoints extended to cover bytes up to
    /// `new_size`. Cancellable; if `cancel` flips, returns `Err(Cancelled)` and
    /// the caller falls back to the prior backend.
    ///
    /// Every boundary at or below the last row's start is settled by bytes the old file
    /// already held (a segment multiple needs only the segment behind it, a line start
    /// only the newline behind it), so the extend rewalks from the checkpoint before
    /// that row and keeps everything below. That re-reads at most one checkpoint
    /// interval, which is what bounds the append. Memory: the checkpoint vec is cloned
    /// (24 bytes each, ~390 K for a 100 M-row file).
    pub fn extend_to(&self, new_size: u64, cancel: &AtomicBool) -> Result<Self, ViewerError> {
        if new_size <= self.total_bytes {
            return Ok(Self {
                path: self.path.clone(),
                total_bytes: new_size,
                file_name: self.file_name.clone(),
                checkpoints: self.checkpoints.clone(),
                total_rows: self.total_rows,
                total_lines: self.total_lines,
                content_start: self.content_start,
                encoding: self.encoding,
            });
        }

        // The row holding the old file's last byte is the only one the append can
        // change, so the rewalk starts at the checkpoint at or before it.
        let resume_at = self.row_start_of(self.total_bytes.saturating_sub(1))?;
        let idx = match self.checkpoints.binary_search_by_key(&resume_at, |cp| cp.offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let resume = self.checkpoints.get(idx).cloned().unwrap_or(Checkpoint {
            row: 0,
            line: 0,
            offset: self.content_start,
        });

        // Keep everything strictly below the resume point; the rewalk re-pushes the
        // checkpoint AT it, since `resume.row` is a multiple of the interval.
        let mut checkpoints: Vec<Checkpoint> = self.checkpoints[..idx.min(self.checkpoints.len())].to_vec();
        let file = File::open(&self.path)?;
        let mut reader = RowReader::new(FileSource::new(file, new_size), self.encoding, self.content_start);
        reader.seek(resume.offset)?;
        let mut rows = resume.row;
        let mut lines = resume.line;
        while let Some(span) = reader.next_span()? {
            if rows.is_multiple_of(INDEX_CHECKPOINT_INTERVAL) {
                checkpoints.push(Checkpoint {
                    row: rows,
                    line: lines,
                    offset: span.start,
                });
            }
            if span.starts_line {
                lines += 1;
            }
            rows += 1;
            if rows.is_multiple_of(INDEX_CHECKPOINT_INTERVAL) && cancel.load(Ordering::Relaxed) {
                return Err(ViewerError::Cancelled);
            }
        }

        Ok(Self {
            path: self.path.clone(),
            total_bytes: new_size,
            file_name: self.file_name.clone(),
            checkpoints,
            total_rows: rows.max(1),
            total_lines: lines.max(1),
            content_start: self.content_start,
            encoding: self.encoding,
        })
    }

    /// The start of the row containing `offset`, through the shared rule.
    fn row_start_of(&self, offset: u64) -> Result<u64, ViewerError> {
        let file = File::open(&self.path)?;
        let mut reader = RowReader::new(
            FileSource::new(file, self.total_bytes),
            self.encoding,
            self.content_start,
        );
        reader.seek(offset)
    }

    /// Find the checkpoint at or before the given ROW.
    fn find_checkpoint(&self, target_row: usize) -> &Checkpoint {
        let idx = match self.checkpoints.binary_search_by_key(&target_row, |cp| cp.row) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        &self.checkpoints[idx]
    }

    /// A row walk positioned on the row at `target_row`, with the physical line number
    /// that row would print.
    ///
    /// ❗ Walks from the checkpoint to the target, so the reader's own cursor lands on
    /// the TARGET's first byte. Reporting the checkpoint's offset instead is what made
    /// every onward seek land short and duplicate a line at each chunk seam.
    fn reader_at_row(&self, target_row: usize) -> Result<(RowReader<FileSource>, u64, usize), ViewerError> {
        let checkpoint = self.find_checkpoint(target_row).clone();
        let file = File::open(&self.path)?;
        let mut reader = RowReader::new(
            FileSource::new(file, self.total_bytes),
            self.encoding,
            self.content_start,
        );
        let mut at = reader.seek(checkpoint.offset)?;
        let mut row = checkpoint.row;
        let mut line = checkpoint.line;
        // At most one checkpoint interval of rows, whatever the file's size.
        while row < target_row {
            let Some(span) = reader.next_span()? else { break };
            if span.starts_line {
                line += 1;
            }
            row += 1;
            at = span.end;
        }
        reader.seek(at)?;
        Ok((reader, at, line))
    }

    /// Which ROW a target names.
    fn resolve_target(&self, target: &SeekTarget) -> Result<usize, ViewerError> {
        let last_row = self.total_rows.saturating_sub(1);
        Ok(match target {
            SeekTarget::Row(n) => (*n).min(last_row),
            // ❗ The row CONTAINING the byte, not the checkpoint before it. Rounding
            // down to a checkpoint threw a byte-offset seek up to 255 rows backwards,
            // and `read_range` steers between chunks by byte offset, so the rounding
            // re-served rows that had already gone out.
            SeekTarget::ByteOffset(offset) => self.row_at_byte(*offset)?.min(last_row),
            SeekTarget::Fraction(f) => {
                let f = f.clamp(0.0, 1.0);
                (f * last_row as f64).round() as usize
            }
        })
    }

    /// The index of the row containing `offset`.
    ///
    /// Binary-searches the checkpoints, then walks at most one interval of rows: the
    /// walk is what makes the answer exact, the checkpoints are what keep it bounded.
    fn row_at_byte(&self, offset: u64) -> Result<usize, ViewerError> {
        let offset = offset.clamp(self.content_start, self.total_bytes);
        let idx = match self.checkpoints.binary_search_by_key(&offset, |cp| cp.offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let Some(checkpoint) = self.checkpoints.get(idx) else {
            return Ok(0);
        };
        let file = File::open(&self.path)?;
        let mut reader = RowReader::new(
            FileSource::new(file, self.total_bytes),
            self.encoding,
            self.content_start,
        );
        reader.seek(checkpoint.offset)?;
        let mut row = checkpoint.row;
        while let Some(span) = reader.next_span()? {
            if offset < span.end || span.end == span.start {
                return Ok(row);
            }
            row += 1;
        }
        Ok(row.saturating_sub(1))
    }
}

impl FileViewerBackend for LineIndexBackend {
    fn extend_to_boxed(&self, new_size: u64, cancel: &AtomicBool) -> Result<Box<dyn FileViewerBackend>, ViewerError> {
        let extended = self.extend_to(new_size, cancel)?;
        Ok(Box::new(extended))
    }

    fn with_encoding(&self, new_encoding: FileEncoding) -> Option<Box<dyn FileViewerBackend>> {
        // Only valid when the new encoding shares byte layout with the current
        // one (same BOM + both ASCII-newline-compatible). The session enforces
        // this via `same_byte_layout` before calling, but check again here so
        // a future caller can't accidentally bypass the rebuild.
        if !super::encoding::same_byte_layout(self.encoding, new_encoding) {
            return None;
        }
        Some(Box::new(Self {
            path: self.path.clone(),
            total_bytes: self.total_bytes,
            file_name: self.file_name.clone(),
            checkpoints: self.checkpoints.clone(),
            total_rows: self.total_rows,
            total_lines: self.total_lines,
            content_start: self.content_start,
            encoding: new_encoding,
        }))
    }

    fn get_lines(&self, target: &SeekTarget, count: usize, cancel: &AtomicBool) -> Result<LineChunk, ViewerError> {
        let target_row = self.resolve_target(target)?;
        let (mut reader, row_offset, line) = self.reader_at_row(target_row)?;
        let collected = collect_rows(&mut reader, Some(line), count, cancel)?;

        Ok(LineChunk {
            rows: collected.rows,
            first_row_number: target_row,
            // ❗ The TARGET row's offset, not the checkpoint's. The old answer sent
            // every onward seek short of where it said it was, which duplicated a line
            // at every chunk seam of a copy or a save over 4 096 rows.
            byte_offset: row_offset,
            end_byte_offset: collected.end_byte_offset,
            end: collected.end,
            total_rows: TotalRows::Exact(self.total_rows),
            total_bytes: self.total_bytes,
        })
    }

    /// Scan the file row by row. See `row_walk::search_rows`: one implementation, shared
    /// with `ByteSeekBackend`, instead of the two copies of the same quadratic,
    /// UTF-16-blind `memchr` loop that stood here.
    fn search(
        &self,
        matcher: &Matcher,
        cancel: &AtomicBool,
        results: &Mutex<Vec<SearchMatch>>,
        progress: &Mutex<u64>,
    ) -> Result<u64, ViewerError> {
        let file = File::open(&self.path)?;
        let mut reader = RowReader::new(
            FileSource::new(file, self.total_bytes),
            self.encoding,
            self.content_start,
        );
        search_rows(&mut reader, matcher, cancel, results, progress)
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            supports_line_seek: true,
            supports_byte_seek: true,
            supports_fraction_seek: true,
            knows_total_lines: true,
        }
    }

    fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    fn total_rows(&self) -> TotalRows {
        TotalRows::Exact(self.total_rows)
    }

    fn total_lines(&self) -> Option<usize> {
        Some(self.total_lines)
    }

    fn file_name(&self) -> &str {
        &self.file_name
    }
}

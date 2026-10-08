//! FullLoadBackend: loads entire file into memory.
//!
//! Best for files under FULL_LOAD_THRESHOLD (1 MB). Provides instant random
//! access by row and fast search since all content is in RAM.
//!
//! ❗ It needs the row rule as much as the streaming backends do: a 900 KB file can
//! still be one minified line, and serving that as a single 900 KB "line" would put
//! the frontend's row cache and the other two backends' grids out of step.

use crate::ignore_poison::IgnorePoison;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use super::encoding::FileEncoding;
use super::row_walk::{self, RowReader, TotalRows, ViewerRow};
use super::rows::SliceSource;
use super::search_matcher::{LineScan, Matcher, scan_line_with_matcher};
use super::{BackendCapabilities, ChunkEnd, FileViewerBackend, LineChunk, SearchMatch, SeekTarget, ViewerError};

pub struct FullLoadBackend {
    /// Every row of the file, in order. Each carries its own byte offset, so a seek is
    /// a binary search and nothing downstream re-derives an offset from string lengths.
    rows: Vec<ViewerRow>,
    /// Absolute offset just past the last row. `total_bytes` with the BOM counted in,
    /// which is what a `RangeEnd::Eof` read walks to.
    end_byte_offset: u64,
    /// Physical lines, for the gutter and for `total_lines`.
    total_lines: usize,
    total_bytes: u64,
    file_name: String,
}

impl FullLoadBackend {
    /// Open with auto-detected encoding. Falls back to UTF-8 on detection IO errors
    /// (the subsequent `decode_line` calls then run through `from_utf8_lossy`, which
    /// is what the viewer used to do before encoding-awareness landed).
    ///
    /// Test-only: production always opens through `open_with_encoding` with an
    /// explicit detected encoding (the session detects once and shares it).
    #[cfg(test)]
    pub fn open(path: &Path) -> Result<Self, ViewerError> {
        let encoding = super::encoding::detect(path).unwrap_or(FileEncoding::Utf8);
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

        let total_bytes = metadata.len();
        let bytes = std::fs::read(path)?;
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        Ok(Self::build_from_bytes(bytes, total_bytes, file_name, encoding))
    }

    /// Split `bytes` into rows through the shared row rule, with absolute byte offsets
    /// in the SOURCE bytes (not the decoded UTF-8). Search and selection flows
    /// downstream of this struct convert UTF-16 offsets via the existing surrogate-safe
    /// clamp; this struct keeps source-byte offsets so range reads against the raw file
    /// still line up.
    ///
    /// The BOM is not content: the first row starts past it, and offsets stay aligned
    /// with the on-disk file.
    fn build_from_bytes(bytes: Vec<u8>, total_bytes: u64, file_name: String, encoding: FileEncoding) -> Self {
        let bom_len = row_walk::content_start(&bytes, encoding);

        let mut reader = RowReader::new(SliceSource::new(&bytes), encoding, bom_len);
        let mut rows: Vec<ViewerRow> = Vec::new();
        let mut end_byte_offset = bom_len;
        let mut next_line = 0usize;
        // ❗ `expect`, not `unwrap_or(None)`: a swallowed error here would end the walk
        // early and hand back a file missing its tail, with nothing saying so.
        while let Some((span, text)) = reader.next_row().expect("a slice source cannot fail to read") {
            let line_number = if span.starts_line {
                let n = next_line;
                next_line += 1;
                Some(n)
            } else {
                None
            };
            rows.push(ViewerRow {
                text,
                byte_offset: span.start,
                continues: span.continues,
                line_number,
            });
            end_byte_offset = span.end;
        }

        Self {
            rows,
            end_byte_offset,
            total_lines: next_line.max(1),
            total_bytes,
            file_name,
        }
    }

    /// `extend_to` doesn't apply to FullLoad: on an append, the session's
    /// `apply_tail_extend` reopens the file instead (FullLoad again, or ByteSeek
    /// past `FULL_LOAD_THRESHOLD`). Calling it is a bug; we panic so the call
    /// site surfaces fast rather than silently dropping the append.
    #[allow(dead_code, reason = "called by session::tail_mode_extend defensively")]
    pub fn extend_to(&self, _new_size: u64, _cancel: &AtomicBool) -> Self {
        unreachable!(
            "FullLoadBackend::extend_to should never be called: sessions escalate to ByteSeek before extending"
        )
    }

    /// Create from in-memory UTF-8 content (for testing). Always opens as UTF-8 with
    /// the legacy split-on-`\n` semantics that pre-encoding tests rely on.
    #[cfg(test)]
    pub fn from_content(content: &str, file_name: &str) -> Self {
        Self::build_from_bytes(
            content.as_bytes().to_vec(),
            content.len() as u64,
            file_name.to_string(),
            FileEncoding::Utf8,
        )
    }

    fn resolve_target(&self, target: &SeekTarget) -> usize {
        match target {
            SeekTarget::Row(n) => (*n).min(self.rows.len().saturating_sub(1)),
            SeekTarget::ByteOffset(offset) => {
                // Binary search for the row containing this byte offset.
                match self.rows.binary_search_by_key(offset, |row| row.byte_offset) {
                    Ok(idx) => idx,
                    Err(idx) => idx.saturating_sub(1),
                }
            }
            SeekTarget::Fraction(f) => {
                let f = f.clamp(0.0, 1.0);
                let max_row = self.rows.len().saturating_sub(1);
                (f * max_row as f64).round() as usize
            }
        }
    }
}

impl FileViewerBackend for FullLoadBackend {
    fn extend_to_boxed(&self, _new_size: u64, _cancel: &AtomicBool) -> Result<Box<dyn FileViewerBackend>, ViewerError> {
        // The session reopens a FullLoad file instead of extending it. Reaching
        // here means the caller skipped that; surface a typed error rather than
        // panicking inside a watcher thread.
        Err(ViewerError::Io {
            message: "FullLoadBackend cannot extend in place; session must escalate first".to_string(),
        })
    }

    fn get_lines(&self, target: &SeekTarget, count: usize, cancel: &AtomicBool) -> Result<LineChunk, ViewerError> {
        let start = self.resolve_target(target);
        let mut end = start;
        let mut taken = 0u64;
        // The budget bounds the answer here too: a file under 1 MB can hold rows a
        // wrap-on viewport would rather not receive in one go, and every caller reads
        // `ChunkEnd` the same way whichever backend served it.
        while end < self.rows.len() && end - start < count {
            // In memory, so never slow; checked anyway, so the flag means the same on
            // every backend.
            if cancel.load(Ordering::Relaxed) {
                return Err(ViewerError::Cancelled);
            }
            taken += self.rows[end].text.len() as u64;
            end += 1;
            if taken >= super::CHUNK_BUDGET_BYTES {
                break;
            }
        }
        let chunk_end = if end >= self.rows.len() {
            ChunkEnd::EndOfFile
        } else if end - start < count {
            ChunkEnd::BudgetReached
        } else {
            ChunkEnd::CountReached
        };

        Ok(LineChunk {
            rows: self.rows[start..end].to_vec(),
            first_row_number: start,
            byte_offset: self.rows.get(start).map_or(self.end_byte_offset, |row| row.byte_offset),
            end_byte_offset: self.rows.get(end).map_or(self.end_byte_offset, |row| row.byte_offset),
            end: chunk_end,
            total_rows: TotalRows::Exact(self.rows.len()),
            total_bytes: self.total_bytes,
        })
    }

    fn search(
        &self,
        matcher: &Matcher,
        cancel: &AtomicBool,
        results: &Mutex<Vec<SearchMatch>>,
        progress: &Mutex<u64>,
    ) -> Result<u64, ViewerError> {
        let mut scanned: u64 = 0;
        let mut limit_reached = false;

        // Row by row, like the other two backends: a match's column then counts from
        // the start of the ROW it sits in, so ⌘F inside a minified line lands somewhere
        // the frontend can scroll to. A needle straddling a segment break is missed,
        // which is inherent to searching rows and is why `SEGMENT_BYTES` is far larger
        // than any query.
        for (row_idx, row) in self.rows.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) || limit_reached {
                break;
            }
            match scan_line_with_matcher(matcher, &row.text, row_idx, row.byte_offset, cancel, results) {
                LineScan::HitLimit => limit_reached = true,
                LineScan::Cancelled => break,
                LineScan::Done => {}
            }
            // ❗ Progress in SOURCE bytes, the units `total_bytes` is in, taken from the
            // NEXT row's offset rather than from this row's decoded length. Decoded
            // UTF-8 is about half the source on a UTF-16 file, so summing it stalled the
            // frontend's progress bar near 50%; and adding a byte per delimiter counted
            // one for the final empty row, pushing the total past the file's size.
            scanned = self
                .rows
                .get(row_idx + 1)
                .map_or(self.end_byte_offset, |next| next.byte_offset);
        }

        *progress.lock_ignore_poison() = scanned;
        Ok(scanned)
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
        TotalRows::Exact(self.rows.len())
    }

    fn total_lines(&self) -> Option<usize> {
        Some(self.total_lines)
    }

    fn file_name(&self) -> &str {
        &self.file_name
    }
}

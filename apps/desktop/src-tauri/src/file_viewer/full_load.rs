//! FullLoadBackend: loads entire file into memory.
//!
//! Best for files under FULL_LOAD_THRESHOLD (1 MB). Provides instant random
//! access by line number and fast search since all content is in RAM.

use crate::ignore_poison::IgnorePoison;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use super::encoding::{FileEncoding, decode_line, detect, find_newlines};
use super::search_matcher::{LineScan, Matcher, scan_line_with_matcher};
use super::{BackendCapabilities, FileViewerBackend, LineChunk, SearchMatch, SeekTarget, ViewerError};

pub struct FullLoadBackend {
    lines: Vec<String>,
    /// Byte offset of each line start (parallel to `lines`).
    line_offsets: Vec<u64>,
    total_bytes: u64,
    file_name: String,
    #[allow(dead_code, reason = "milestone-3 watcher/tail extends usage")]
    encoding: FileEncoding,
}

impl FullLoadBackend {
    /// Open with auto-detected encoding. Falls back to UTF-8 on detection IO errors
    /// (the subsequent `decode_line` calls then run through `from_utf8_lossy`, which
    /// is what the viewer used to do before encoding-awareness landed).
    #[allow(dead_code, reason = "milestone-3 watcher/tail extends usage")]
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

        let total_bytes = metadata.len();
        let bytes = std::fs::read(path)?;
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        Ok(Self::build_from_bytes(bytes, total_bytes, file_name, encoding))
    }

    /// Split `bytes` into per-encoding lines, populating `line_offsets` with absolute
    /// byte offsets in the SOURCE bytes (not the decoded UTF-8). Search and selection
    /// flows downstream of this struct convert UTF-16 offsets via the existing
    /// surrogate-safe clamp; this struct keeps source-byte offsets so range reads
    /// against the raw file still line up.
    fn build_from_bytes(bytes: Vec<u8>, total_bytes: u64, file_name: String, encoding: FileEncoding) -> Self {
        // Strip the leading BOM if present so the first line doesn't surface it as
        // visible content. The byte offset accounting keeps the BOM bytes in the
        // count (offsets stay aligned with the on-disk file).
        let bom_len = if bytes.starts_with(encoding.bom_bytes()) {
            encoding.bom_bytes().len()
        } else {
            0
        };
        let scan = &bytes[bom_len..];
        let newlines = find_newlines(scan, encoding);

        let mut lines: Vec<String> = Vec::with_capacity(newlines.len() + 1);
        let mut line_offsets: Vec<u64> = Vec::with_capacity(newlines.len() + 1);
        let mut start: usize = 0;
        for nl in &newlines {
            line_offsets.push((bom_len + start) as u64);
            // The byte that starts the newline pair, and the byte just after the pair.
            //   ASCII-compatible: pair = [0x0A], starts at nl, ends at nl + 1.
            //   UTF-16 LE: pair = [0x0A, 0x00] starting at nl, ending at nl + 2.
            //   UTF-16 BE: pair = [0x00, 0x0A] starting at nl - 1, ending at nl + 1.
            let (pair_start, next_start) = match encoding {
                FileEncoding::Utf16Le => (*nl, nl + 2),
                FileEncoding::Utf16Be => (nl - 1, nl + 1),
                _ => (*nl, nl + 1),
            };
            lines.push(decode_line(&scan[start..pair_start], encoding));
            start = next_start;
        }
        // Trailing partial line (or whole content if no newlines).
        if start < scan.len() {
            line_offsets.push((bom_len + start) as u64);
            lines.push(decode_line(&scan[start..], encoding));
        } else if lines.is_empty() {
            // Empty file → one empty line.
            line_offsets.push(bom_len as u64);
            lines.push(String::new());
        } else if newlines.last().is_some() {
            // File ends with a newline → trailing empty line, matching split('\n') legacy.
            line_offsets.push(bom_len as u64 + scan.len() as u64);
            lines.push(String::new());
        }

        Self {
            lines,
            line_offsets,
            total_bytes,
            file_name,
            encoding,
        }
    }

    #[allow(dead_code, reason = "milestone-3 watcher/tail extends usage")]
    pub fn encoding(&self) -> FileEncoding {
        self.encoding
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
            SeekTarget::Line(n) => (*n).min(self.lines.len().saturating_sub(1)),
            SeekTarget::ByteOffset(offset) => {
                // Binary search for the line containing this byte offset
                match self.line_offsets.binary_search(offset) {
                    Ok(idx) => idx,
                    Err(idx) => idx.saturating_sub(1),
                }
            }
            SeekTarget::Fraction(f) => {
                let f = f.clamp(0.0, 1.0);
                let max_line = self.lines.len().saturating_sub(1);
                (f * max_line as f64).round() as usize
            }
        }
    }
}

impl FileViewerBackend for FullLoadBackend {
    fn get_lines(&self, target: &SeekTarget, count: usize) -> Result<LineChunk, ViewerError> {
        let start = self.resolve_target(target);
        let end = (start + count).min(self.lines.len());
        let chunk_lines: Vec<String> = self.lines[start..end].to_vec();

        Ok(LineChunk {
            lines: chunk_lines,
            first_line_number: start,
            byte_offset: self.line_offsets.get(start).copied().unwrap_or(0),
            total_lines: Some(self.lines.len()),
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

        for (line_idx, line) in self.lines.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) || limit_reached {
                break;
            }
            match scan_line_with_matcher(matcher, line, line_idx, self.line_offsets[line_idx], cancel, results) {
                LineScan::HitLimit => limit_reached = true,
                LineScan::Cancelled => break,
                LineScan::Done => {}
            }
            scanned += line.len() as u64 + 1; // +1 for newline
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

    fn total_lines(&self) -> Option<usize> {
        Some(self.lines.len())
    }

    fn file_name(&self) -> &str {
        &self.file_name
    }
}

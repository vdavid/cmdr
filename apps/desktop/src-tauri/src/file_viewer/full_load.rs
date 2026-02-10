//! FullLoadBackend — loads entire file into memory.
//!
//! Best for files under FULL_LOAD_THRESHOLD (1 MB). Provides instant random
//! access by line number and fast search since all content is in RAM.

use crate::ignore_poison::IgnorePoison;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use super::{BackendCapabilities, FileViewerBackend, LineChunk, SearchMatch, SeekTarget, ViewerError};

pub struct FullLoadBackend {
    lines: Vec<String>,
    /// Byte offset of each line start (parallel to `lines`).
    line_offsets: Vec<u64>,
    total_bytes: u64,
    file_name: String,
}

impl FullLoadBackend {
    pub fn open(path: &Path) -> Result<Self, ViewerError> {
        let metadata = std::fs::metadata(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => ViewerError::NotFound(path.display().to_string()),
            _ => ViewerError::from(e),
        })?;
        if metadata.is_dir() {
            return Err(ViewerError::IsDirectory);
        }

        let total_bytes = metadata.len();
        let bytes = std::fs::read(path)?;
        let content = String::from_utf8_lossy(&bytes);

        let mut lines = Vec::new();
        let mut line_offsets = Vec::new();
        let mut offset: u64 = 0;

        for line in content.split('\n') {
            line_offsets.push(offset);
            lines.push(line.to_string());
            // +1 for the '\n' delimiter (even if last line has none, offset won't be used beyond)
            offset += line.len() as u64 + 1;
        }

        // If file ends without newline and content is non-empty, the split gives correct result.
        // If file is empty, we still want at least one empty line.
        if lines.is_empty() {
            lines.push(String::new());
            line_offsets.push(0);
        }

        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());

        Ok(Self {
            lines,
            line_offsets,
            total_bytes,
            file_name,
        })
    }

    /// Create from in-memory content (for testing).
    #[cfg(test)]
    pub fn from_content(content: &str, file_name: &str) -> Self {
        let total_bytes = content.len() as u64;
        let mut lines = Vec::new();
        let mut line_offsets = Vec::new();
        let mut offset: u64 = 0;

        for line in content.split('\n') {
            line_offsets.push(offset);
            lines.push(line.to_string());
            offset += line.len() as u64 + 1;
        }

        if lines.is_empty() {
            lines.push(String::new());
            line_offsets.push(0);
        }

        Self {
            lines,
            line_offsets,
            total_bytes,
            file_name: file_name.to_string(),
        }
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

    fn search(&self, query: &str, cancel: &AtomicBool, results: &Mutex<Vec<SearchMatch>>) -> Result<u64, ViewerError> {
        let query_lower = query.to_lowercase();
        let mut scanned: u64 = 0;

        for (line_idx, line) in self.lines.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }

            let line_lower = line.to_lowercase();
            let mut search_start = 0;
            while let Some(pos) = line_lower[search_start..].find(&query_lower) {
                let col_bytes = search_start + pos;
                let col_utf16: usize = line_lower[..col_bytes].chars().map(|c| c.len_utf16()).sum();
                let len_utf16: usize = query_lower.chars().map(|c| c.len_utf16()).sum();
                let mut matches = results.lock_ignore_poison();
                matches.push(SearchMatch {
                    line: line_idx,
                    column: col_utf16,
                    length: len_utf16,
                });
                search_start = col_bytes + query_lower.len();
            }

            scanned += line.len() as u64 + 1; // +1 for newline
        }

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

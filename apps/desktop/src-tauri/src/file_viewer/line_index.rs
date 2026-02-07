//! LineIndexBackend — sparse line-offset index for efficient line-based seeking.
//!
//! Stores byte offsets every INDEX_CHECKPOINT_INTERVAL lines (256 by default).
//! Memory: O(total_lines / 256) — a 10M-line file uses ~40 KB of index.
//!
//! The index is built by scanning the file for newlines using memchr (SIMD-accelerated).
//! After scanning, supports O(1) line-based seeking via the checkpoint array.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use memchr::memchr;

use super::{
    BackendCapabilities, FileViewerBackend, INDEX_CHECKPOINT_INTERVAL, LineChunk, SearchMatch, SeekTarget, ViewerError,
};

/// A checkpoint in the line index: (line_number, byte_offset).
#[derive(Debug, Clone)]
struct Checkpoint {
    line: usize,
    offset: u64,
}

pub struct LineIndexBackend {
    path: std::path::PathBuf,
    total_bytes: u64,
    file_name: String,
    /// Sparse index: one checkpoint every INDEX_CHECKPOINT_INTERVAL lines.
    checkpoints: Vec<Checkpoint>,
    /// Total lines discovered during scan.
    total_lines: usize,
}

impl LineIndexBackend {
    /// Build the line index by scanning the file. This is blocking and should be run
    /// in a background thread for large files. Checks `cancel` periodically.
    pub fn open(path: &Path, cancel: &AtomicBool) -> Result<Self, ViewerError> {
        let metadata = std::fs::metadata(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => ViewerError::NotFound(path.display().to_string()),
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

        // Scan the file to build the sparse line index
        let mut file = File::open(path)?;
        let mut checkpoints = Vec::new();
        let chunk_size: usize = 256 * 1024; // 256 KB scan buffer
        let mut buf = vec![0u8; chunk_size];
        let mut line_number: usize = 0;
        let mut byte_offset: u64 = 0;

        // First line always starts at offset 0
        checkpoints.push(Checkpoint { line: 0, offset: 0 });

        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(ViewerError::Io("Scan cancelled".to_string()));
            }

            let bytes_read = file.read(&mut buf)?;
            if bytes_read == 0 {
                break;
            }

            let data = &buf[..bytes_read];
            let mut pos = 0;

            while pos < data.len() {
                if let Some(nl_pos) = memchr(b'\n', &data[pos..]) {
                    line_number += 1;
                    let nl_byte_offset = byte_offset + (pos + nl_pos) as u64 + 1;

                    // Store checkpoint every N lines
                    if line_number.is_multiple_of(INDEX_CHECKPOINT_INTERVAL) {
                        checkpoints.push(Checkpoint {
                            line: line_number,
                            offset: nl_byte_offset,
                        });
                    }

                    pos += nl_pos + 1;
                } else {
                    break;
                }
            }

            byte_offset += bytes_read as u64;
        }

        // total_lines is line_number + 1 (for the last line, which may not end with \n)
        // But if the file ends with \n, the last "line" is empty — we still count it.
        let total_lines = line_number + 1;

        Ok(Self {
            path: path.to_path_buf(),
            total_bytes,
            file_name,
            checkpoints,
            total_lines,
        })
    }

    /// Find the checkpoint at or before the given line number.
    fn find_checkpoint(&self, target_line: usize) -> &Checkpoint {
        // Binary search for the largest checkpoint with line <= target_line
        let idx = match self.checkpoints.binary_search_by_key(&target_line, |cp| cp.line) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        &self.checkpoints[idx]
    }

    /// Read forward from a byte offset, skipping `lines_to_skip` lines,
    /// then returning the next `count` lines.
    fn read_lines_from_checkpoint(
        &self,
        start_offset: u64,
        lines_to_skip: usize,
        count: usize,
    ) -> Result<Vec<String>, ViewerError> {
        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(start_offset))?;

        let chunk_size: usize = 64 * 1024;
        let mut buf = vec![0u8; chunk_size];
        let mut lines = Vec::new();
        let mut skipped: usize = 0;
        let mut leftover = Vec::new();

        'outer: loop {
            let bytes_read = file.read(&mut buf)?;
            if bytes_read == 0 {
                break;
            }

            let mut combined = Vec::new();
            let data: &[u8] = if leftover.is_empty() {
                &buf[..bytes_read]
            } else {
                combined.reserve(leftover.len() + bytes_read);
                combined.extend_from_slice(&leftover);
                combined.extend_from_slice(&buf[..bytes_read]);
                leftover.clear();
                &combined
            };

            let mut pos = 0;
            while pos < data.len() {
                if let Some(nl_pos) = memchr(b'\n', &data[pos..]) {
                    if skipped < lines_to_skip {
                        skipped += 1;
                        pos += nl_pos + 1;
                        continue;
                    }

                    let line_bytes = &data[pos..pos + nl_pos];
                    lines.push(String::from_utf8_lossy(line_bytes).into_owned());
                    pos += nl_pos + 1;

                    if lines.len() >= count {
                        break 'outer;
                    }
                } else {
                    leftover.extend_from_slice(&data[pos..]);
                    continue 'outer;
                }
            }
        }

        // Handle last line without newline
        if !leftover.is_empty() && lines.len() < count && skipped >= lines_to_skip {
            lines.push(String::from_utf8_lossy(&leftover).into_owned());
        }

        Ok(lines)
    }

    fn resolve_target(&self, target: &SeekTarget) -> usize {
        match target {
            SeekTarget::Line(n) => (*n).min(self.total_lines.saturating_sub(1)),
            SeekTarget::ByteOffset(offset) => {
                // Find the checkpoint closest to this byte offset
                let idx = match self.checkpoints.binary_search_by_key(offset, |cp| cp.offset) {
                    Ok(i) => i,
                    Err(i) => i.saturating_sub(1),
                };
                self.checkpoints[idx].line
            }
            SeekTarget::Fraction(f) => {
                let f = f.clamp(0.0, 1.0);
                let max_line = self.total_lines.saturating_sub(1);
                (f * max_line as f64).round() as usize
            }
        }
    }
}

impl FileViewerBackend for LineIndexBackend {
    fn get_lines(&self, target: &SeekTarget, count: usize) -> Result<LineChunk, ViewerError> {
        let target_line = self.resolve_target(target);
        let checkpoint = self.find_checkpoint(target_line);
        let lines_to_skip = target_line - checkpoint.line;

        let lines = self.read_lines_from_checkpoint(checkpoint.offset, lines_to_skip, count)?;

        // Calculate byte offset of the target line (approximate — it's the checkpoint offset)
        let byte_offset = checkpoint.offset;

        Ok(LineChunk {
            lines,
            first_line_number: target_line,
            byte_offset,
            total_lines: Some(self.total_lines),
            total_bytes: self.total_bytes,
        })
    }

    fn search(&self, query: &str, cancel: &AtomicBool, results: &Mutex<Vec<SearchMatch>>) -> Result<u64, ViewerError> {
        // Stream through file in 1 MB chunks, same as ByteSeekBackend
        let query_lower = query.to_lowercase();
        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(0))?;

        let chunk_size: usize = 1024 * 1024;
        let mut buf = vec![0u8; chunk_size];
        let mut line_number: usize = 0;
        let mut scanned: u64 = 0;
        let mut leftover = Vec::new();

        loop {
            if cancel.load(Ordering::Relaxed) {
                break;
            }

            let bytes_read = file.read(&mut buf)?;
            if bytes_read == 0 {
                break;
            }

            let mut combined = Vec::new();
            let data: &[u8] = if leftover.is_empty() {
                &buf[..bytes_read]
            } else {
                combined.reserve(leftover.len() + bytes_read);
                combined.extend_from_slice(&leftover);
                combined.extend_from_slice(&buf[..bytes_read]);
                leftover.clear();
                &combined
            };

            let mut pos = 0;
            while pos < data.len() {
                if cancel.load(Ordering::Relaxed) {
                    return Ok(scanned);
                }

                if let Some(nl_pos) = memchr(b'\n', &data[pos..]) {
                    let line_bytes = &data[pos..pos + nl_pos];
                    let line = String::from_utf8_lossy(line_bytes);
                    let line_lower = line.to_lowercase();

                    let mut search_start = 0;
                    while let Some(match_pos) = line_lower[search_start..].find(&query_lower) {
                        let col = search_start + match_pos;
                        let mut matches = results.lock().unwrap_or_else(|e| e.into_inner());
                        matches.push(SearchMatch {
                            line: line_number,
                            column: col,
                            length: query.len(),
                        });
                        search_start = col + 1;
                    }

                    scanned += (nl_pos + 1) as u64;
                    pos += nl_pos + 1;
                    line_number += 1;
                } else {
                    leftover.extend_from_slice(&data[pos..]);
                    break;
                }
            }
        }

        // Handle last line
        if !leftover.is_empty() {
            let line = String::from_utf8_lossy(&leftover);
            let line_lower = line.to_lowercase();
            let mut search_start = 0;
            while let Some(match_pos) = line_lower[search_start..].find(&query_lower) {
                let col = search_start + match_pos;
                let mut matches = results.lock().unwrap_or_else(|e| e.into_inner());
                matches.push(SearchMatch {
                    line: line_number,
                    column: col,
                    length: query.len(),
                });
                search_start = col + 1;
            }
            scanned += leftover.len() as u64;
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
        Some(self.total_lines)
    }

    fn file_name(&self) -> &str {
        &self.file_name
    }
}

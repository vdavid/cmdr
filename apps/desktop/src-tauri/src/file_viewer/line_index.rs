//! LineIndexBackend: sparse line-offset index for efficient line-based seeking.
//!
//! Stores byte offsets every INDEX_CHECKPOINT_INTERVAL lines (256 by default).
//! Memory: O(total_lines / 256); a 10M-line file uses ~40 KB of index.
//!
//! The index is built by scanning the file for newlines using memchr (SIMD-accelerated).
//! After scanning, supports O(1) line-based seeking via the checkpoint array.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::ignore_poison::IgnorePoison;
use memchr::memchr;

use super::encoding::{FileEncoding, NewlineScanner, decode_line, detect};
use super::search_matcher::{LineScan, Matcher, scan_line_with_matcher};
use super::{
    BackendCapabilities, FileViewerBackend, INDEX_CHECKPOINT_INTERVAL, LineChunk, SearchMatch, SeekTarget, ViewerError,
};

/// A checkpoint in the line index: (line_number, byte_offset).
#[derive(Debug, Clone)]
struct Checkpoint {
    line: usize,
    /// Absolute file offset of the FIRST byte of the line at index `line`.
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
    encoding: FileEncoding,
}

impl LineIndexBackend {
    /// Build the line index by scanning the file. Auto-detects encoding.
    #[allow(dead_code, reason = "milestone-3 watcher/tail extends usage")]
    pub fn open(path: &Path, cancel: &AtomicBool) -> Result<Self, ViewerError> {
        let encoding = detect(path).unwrap_or(FileEncoding::Utf8);
        Self::open_with_encoding(path, encoding, cancel)
    }

    pub fn open_with_encoding(path: &Path, encoding: FileEncoding, cancel: &AtomicBool) -> Result<Self, ViewerError> {
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

        // Scan the file to build the sparse line index, dispatching to the encoding-aware
        // newline scanner. The scanner emits absolute file offsets of `0x0A` bytes that
        // are part of a `U+000A` code unit; we convert that to "start of NEXT line" by
        // adding 1 (ASCII) or 2 (UTF-16) and adding 1 to `line_number`.
        let mut file = File::open(path)?;
        // For UTF-16, skip the BOM bytes in our line-numbering accounting but record
        // the first line's offset as the byte just past the BOM.
        let bom_len = encoding.bom_bytes().len() as u64;
        let first_line_offset = if total_bytes >= bom_len { bom_len } else { 0 };

        let mut checkpoints = Vec::new();
        let chunk_size: usize = 256 * 1024;
        let mut buf = vec![0u8; chunk_size];
        let mut line_number: usize = 0;
        let mut scanner = NewlineScanner::new(encoding, 0);

        // First line always starts at the byte just past the BOM.
        checkpoints.push(Checkpoint {
            line: 0,
            offset: first_line_offset,
        });

        let le = matches!(encoding, FileEncoding::Utf16Le);
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(ViewerError::Cancelled);
            }

            let bytes_read = file.read(&mut buf)?;
            if bytes_read == 0 {
                break;
            }

            let chunk = &buf[..bytes_read];
            // Collect newline offsets from the scanner (it owns absolute-offset state).
            let mut hits: Vec<u64> = Vec::new();
            scanner.feed(chunk, |off| hits.push(off));

            for nl in &hits {
                line_number += 1;
                // Next line starts past the newline code unit.
                //   ASCII-compatible: nl is the `0x0A` byte; next starts at nl + 1.
                //   UTF-16 LE: nl is the low byte (0x0A) starting the pair; next at nl + 2.
                //   UTF-16 BE: nl is the low byte (0x0A) at offset nl, pair starts at nl - 1;
                //     next at nl + 1 == (nl - 1) + 2.
                let next_line_offset = if encoding.is_ascii_newline_compatible() {
                    *nl + 1
                } else if le {
                    *nl + 2
                } else {
                    *nl + 1
                };
                if line_number.is_multiple_of(INDEX_CHECKPOINT_INTERVAL) {
                    checkpoints.push(Checkpoint {
                        line: line_number,
                        offset: next_line_offset,
                    });
                }
            }
        }

        let total_lines = line_number + 1;

        Ok(Self {
            path: path.to_path_buf(),
            total_bytes,
            file_name,
            checkpoints,
            total_lines,
            encoding,
        })
    }

    #[allow(dead_code, reason = "milestone-3 watcher/tail extends usage")]
    pub fn encoding(&self) -> FileEncoding {
        self.encoding
    }

    /// Returns a fresh backend with checkpoints extended to cover bytes up to
    /// `new_size`. Cancellable; if `cancel` flips, returns `Err(Cancelled)` and
    /// the caller falls back to the prior backend.
    ///
    /// Cost: opens the file, seeks to `self.total_bytes`, scans only the new
    /// range. Memory: the checkpoint vec is cloned (O(checkpoints), cheap — 16
    /// bytes per checkpoint, ~390 K for a 100 M-line file).
    #[allow(dead_code, reason = "milestone-3 watcher/tail extends usage")]
    pub fn extend_to(&self, new_size: u64, cancel: &AtomicBool) -> Result<Self, ViewerError> {
        if new_size <= self.total_bytes {
            return Ok(Self {
                path: self.path.clone(),
                total_bytes: new_size,
                file_name: self.file_name.clone(),
                checkpoints: self.checkpoints.clone(),
                total_lines: self.total_lines,
                encoding: self.encoding,
            });
        }
        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(self.total_bytes))?;

        let mut checkpoints = self.checkpoints.clone();
        // total_lines counts the trailing-after-last-`\n` virtual line as +1; reverse it
        // so we scan from the actual last-newline boundary.
        let mut line_number = self.total_lines.saturating_sub(1);
        let mut scanner = NewlineScanner::new(self.encoding, self.total_bytes);

        let chunk_size: usize = 256 * 1024;
        let mut buf = vec![0u8; chunk_size];
        let le = matches!(self.encoding, FileEncoding::Utf16Le);
        let mut remaining = new_size - self.total_bytes;
        while remaining > 0 {
            if cancel.load(Ordering::Relaxed) {
                return Err(ViewerError::Cancelled);
            }
            let want = remaining.min(buf.len() as u64) as usize;
            let bytes_read = file.read(&mut buf[..want])?;
            if bytes_read == 0 {
                break;
            }
            let chunk = &buf[..bytes_read];
            let mut hits: Vec<u64> = Vec::new();
            scanner.feed(chunk, |off| hits.push(off));
            for nl in &hits {
                line_number += 1;
                let next_line_offset = if self.encoding.is_ascii_newline_compatible() {
                    *nl + 1
                } else if le {
                    *nl + 2
                } else {
                    *nl + 1
                };
                if line_number.is_multiple_of(INDEX_CHECKPOINT_INTERVAL) {
                    checkpoints.push(Checkpoint {
                        line: line_number,
                        offset: next_line_offset,
                    });
                }
            }
            remaining -= bytes_read as u64;
        }

        Ok(Self {
            path: self.path.clone(),
            total_bytes: new_size,
            file_name: self.file_name.clone(),
            checkpoints,
            total_lines: line_number + 1,
            encoding: self.encoding,
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
        if self.encoding.is_ascii_newline_compatible() {
            self.read_lines_ascii_from(start_offset, lines_to_skip, count)
        } else {
            self.read_lines_utf16_from(start_offset, lines_to_skip, count)
        }
    }

    fn read_lines_ascii_from(
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
                    lines.push(decode_line(line_bytes, self.encoding));
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
            lines.push(decode_line(&leftover, self.encoding));
        }

        Ok(lines)
    }

    fn read_lines_utf16_from(
        &self,
        start_offset: u64,
        lines_to_skip: usize,
        count: usize,
    ) -> Result<Vec<String>, ViewerError> {
        let mut file = File::open(&self.path)?;
        let aligned_start = start_offset & !1;
        file.seek(SeekFrom::Start(aligned_start))?;

        let mut lines: Vec<String> = Vec::with_capacity(count);
        let mut scanner = NewlineScanner::new(self.encoding, aligned_start);
        let mut accum: Vec<u8> = Vec::new();
        let mut line_start: u64 = aligned_start;
        let mut skipped: usize = 0;

        let le = matches!(self.encoding, FileEncoding::Utf16Le);

        let chunk_size: usize = 64 * 1024;
        let mut buf = vec![0u8; chunk_size];

        while lines.len() < count {
            let bytes_read = file.read(&mut buf)?;
            if bytes_read == 0 {
                break;
            }
            let chunk = &buf[..bytes_read];
            let mut hits: Vec<u64> = Vec::new();
            scanner.feed(chunk, |off| hits.push(off));
            accum.extend_from_slice(chunk);

            for nl_byte_off in &hits {
                if lines.len() >= count {
                    break;
                }
                let pair_start = if le { *nl_byte_off } else { nl_byte_off - 1 };
                let next_start = pair_start + 2;
                let line_len_bytes = (pair_start - line_start) as usize;
                if skipped < lines_to_skip {
                    skipped += 1;
                } else {
                    let line = decode_line(&accum[..line_len_bytes], self.encoding);
                    lines.push(line);
                }
                let drain = (next_start - line_start) as usize;
                accum.drain(..drain);
                line_start = next_start;
            }
        }

        // Trailing partial line.
        if !accum.is_empty() && lines.len() < count && skipped >= lines_to_skip {
            lines.push(decode_line(&accum, self.encoding));
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
    fn extend_to_boxed(&self, new_size: u64, cancel: &AtomicBool) -> Result<Box<dyn FileViewerBackend>, ViewerError> {
        let extended = self.extend_to(new_size, cancel)?;
        Ok(Box::new(extended))
    }

    fn get_lines(&self, target: &SeekTarget, count: usize) -> Result<LineChunk, ViewerError> {
        let target_line = self.resolve_target(target);
        let checkpoint = self.find_checkpoint(target_line);
        let lines_to_skip = target_line - checkpoint.line;

        let lines = self.read_lines_from_checkpoint(checkpoint.offset, lines_to_skip, count)?;

        // Calculate byte offset of the target line (approximate; it's the checkpoint offset)
        let byte_offset = checkpoint.offset;

        Ok(LineChunk {
            lines,
            first_line_number: target_line,
            byte_offset,
            total_lines: Some(self.total_lines),
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
        // Stream through file in 1 MB chunks, same as ByteSeekBackend.
        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(0))?;

        let chunk_size: usize = 1024 * 1024;
        let mut buf = vec![0u8; chunk_size];
        let mut line_number: usize = 0;
        let mut scanned: u64 = 0;
        let mut line_byte_offset: u64 = 0;
        let mut leftover = Vec::new();
        let mut limit_reached = false;

        loop {
            if cancel.load(Ordering::Relaxed) || limit_reached {
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
                if cancel.load(Ordering::Relaxed) || limit_reached {
                    *progress.lock_ignore_poison() = scanned;
                    return Ok(scanned);
                }

                if let Some(nl_pos) = memchr(b'\n', &data[pos..]) {
                    let line_bytes = &data[pos..pos + nl_pos];
                    let line = decode_line(line_bytes, self.encoding);
                    match scan_line_with_matcher(matcher, &line, line_number, line_byte_offset, cancel, results) {
                        LineScan::HitLimit => limit_reached = true,
                        LineScan::Cancelled => {
                            *progress.lock_ignore_poison() = scanned;
                            return Ok(scanned);
                        }
                        LineScan::Done => {}
                    }

                    scanned += (nl_pos + 1) as u64;
                    pos += nl_pos + 1;
                    line_byte_offset = scanned;
                    line_number += 1;
                } else {
                    leftover.extend_from_slice(&data[pos..]);
                    break;
                }
            }

            // Update progress after each chunk so the frontend can show real progress
            *progress.lock_ignore_poison() = scanned;
        }

        // Handle last line (only reached if limit not hit; loop breaks early otherwise)
        if !leftover.is_empty() {
            let line = decode_line(&leftover, self.encoding);
            let _ = scan_line_with_matcher(matcher, &line, line_number, line_byte_offset, cancel, results);
            scanned += leftover.len() as u64;
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
        Some(self.total_lines)
    }

    fn file_name(&self) -> &str {
        &self.file_name
    }
}

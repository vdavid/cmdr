//! ByteSeekBackend: byte-offset seeking with no pre-scan.
//!
//! Opens the file and can immediately serve lines at any byte position.
//! Scans backward up to MAX_BACKWARD_SCAN bytes to find a newline boundary.
//! If no newline is found (for example, in a binary file), treats the seek position as a line
//! start.
//!
//! Supports Fraction seeking by multiplying fraction × total_bytes.
//! Does NOT support Line seeking (use LineIndexBackend for that).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::ignore_poison::IgnorePoison;
use log::debug;
use memchr::memchr;

use super::encoding::{FileEncoding, NewlineScanner, decode_line, detect};
use super::search_matcher::{LineScan, Matcher, scan_line_with_matcher};
use super::{
    BackendCapabilities, FileViewerBackend, LineChunk, MAX_BACKWARD_SCAN, SearchMatch, SeekTarget, ViewerError,
};

pub struct ByteSeekBackend {
    path: std::path::PathBuf,
    total_bytes: u64,
    file_name: String,
    encoding: FileEncoding,
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

        Ok(Self {
            path: path.to_path_buf(),
            total_bytes: metadata.len(),
            file_name,
            encoding,
        })
    }

    #[allow(dead_code, reason = "milestone-3 watcher/tail extends usage")]
    pub fn encoding(&self) -> FileEncoding {
        self.encoding
    }

    /// Returns a fresh backend with `total_bytes = new_size`. The backend is
    /// immutable; tail-mode extension produces a new instance and `ArcSwap`s it
    /// into place. Cancellable for symmetry with `LineIndexBackend::extend_to`.
    #[allow(dead_code, reason = "milestone-3 watcher/tail extends usage")]
    pub fn extend_to(&self, new_size: u64, _cancel: &AtomicBool) -> Self {
        Self {
            path: self.path.clone(),
            total_bytes: new_size,
            file_name: self.file_name.clone(),
            encoding: self.encoding,
        }
    }

    /// Given a byte offset, scan backward to find the start of the line containing that offset.
    /// Returns the byte offset of the line start.
    fn find_line_start(&self, file: &mut File, offset: u64) -> std::io::Result<u64> {
        if offset == 0 {
            return Ok(0);
        }

        // How far back can we go?
        let scan_len = (offset as usize).min(MAX_BACKWARD_SCAN);
        let scan_start = offset - scan_len as u64;

        file.seek(SeekFrom::Start(scan_start))?;
        let mut buf = vec![0u8; scan_len];
        let bytes_read = file.read(&mut buf)?;
        let buf = &buf[..bytes_read];

        // Search backward for '\n'; the line starts right after the last newline before offset.
        // We search in reverse using memchr on the reversed slice approach.
        if let Some(pos) = buf.iter().rposition(|&b| b == b'\n') {
            // Line starts right after this newline
            Ok(scan_start + pos as u64 + 1)
        } else {
            // No newline found within MAX_BACKWARD_SCAN, treat scan_start as line start
            // (or if scan_start == 0, the file starts here)
            Ok(scan_start)
        }
    }

    /// Read `count` lines starting from `byte_offset`.
    /// Returns the lines and the byte offset just past the last line read.
    fn read_lines_from(&self, start_offset: u64, count: usize) -> Result<(Vec<String>, u64), ViewerError> {
        if self.encoding.is_ascii_newline_compatible() {
            self.read_lines_ascii(start_offset, count)
        } else {
            self.read_lines_utf16(start_offset, count)
        }
    }

    fn read_lines_ascii(&self, start_offset: u64, count: usize) -> Result<(Vec<String>, u64), ViewerError> {
        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(start_offset))?;

        let mut lines = Vec::with_capacity(count);
        let mut current_offset = start_offset;

        // Read in chunks for efficiency
        let chunk_size: usize = 64 * 1024; // 64 KB read buffer
        let mut buf = vec![0u8; chunk_size];
        let mut leftover = Vec::new();

        'outer: while lines.len() < count && current_offset < self.total_bytes {
            let bytes_read = file.read(&mut buf)?;
            if bytes_read == 0 {
                break;
            }

            // Prepend any leftover from previous chunk
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
            while pos < data.len() && lines.len() < count {
                if let Some(nl_pos) = memchr(b'\n', &data[pos..]) {
                    let line_bytes = &data[pos..pos + nl_pos];
                    let line = decode_line(line_bytes, self.encoding);
                    lines.push(line);
                    current_offset += (nl_pos + 1) as u64; // +1 for newline
                    pos += nl_pos + 1;
                } else {
                    // No newline in remaining data, save as leftover
                    leftover.extend_from_slice(&data[pos..]);
                    continue 'outer;
                }
            }
        }

        // If there's leftover data (last line without newline), add it
        if !leftover.is_empty() && lines.len() < count {
            let line = decode_line(&leftover, self.encoding);
            current_offset += leftover.len() as u64;
            lines.push(line);
        }

        Ok((lines, current_offset))
    }

    /// UTF-16 path: drain the file through a `NewlineScanner` and accumulate
    /// pair-aligned line slices. The scanner's `feed` callback emits absolute
    /// file offsets, so we keep a `pair_start` tracking the start of the current
    /// line's pair-aligned region and slice each completed line at the byte
    /// before the newline code unit.
    fn read_lines_utf16(&self, start_offset: u64, count: usize) -> Result<(Vec<String>, u64), ViewerError> {
        let mut file = File::open(&self.path)?;
        // For UTF-16, every code unit is 2 bytes. Align the start offset down to an
        // even boundary in case the caller passed a misaligned byte offset (the
        // backward-scan-for-newline below already returns an even boundary, but
        // an explicit ByteOffset target may not).
        let aligned_start = start_offset & !1;
        file.seek(SeekFrom::Start(aligned_start))?;

        let chunk_size: usize = 64 * 1024;
        let mut buf = vec![0u8; chunk_size];
        let mut scanner = NewlineScanner::new(self.encoding, aligned_start);
        let mut accum: Vec<u8> = Vec::new(); // Bytes from `line_start` to scanner cursor.
        let mut line_start: u64 = aligned_start;
        let mut lines: Vec<String> = Vec::with_capacity(count);
        let mut current_offset = aligned_start;

        let le = matches!(self.encoding, FileEncoding::Utf16Le);

        while lines.len() < count {
            let bytes_read = file.read(&mut buf)?;
            if bytes_read == 0 {
                break;
            }
            let chunk = &buf[..bytes_read];

            // Collect newline offsets, then process between hits without re-feeding.
            let mut hits: Vec<u64> = Vec::new();
            scanner.feed(chunk, |off| hits.push(off));
            accum.extend_from_slice(chunk);
            current_offset += bytes_read as u64;

            for nl_byte_off in &hits {
                if lines.len() >= count {
                    break;
                }
                // Pair start: LE = nl_byte_off, BE = nl_byte_off - 1
                let pair_start = if le { *nl_byte_off } else { nl_byte_off - 1 };
                // Bytes constituting this line (relative to accum start, which is
                // line_start).
                let line_len_bytes = (pair_start - line_start) as usize;
                let line = decode_line(&accum[..line_len_bytes], self.encoding);
                lines.push(line);
                // Skip past the 2-byte newline code unit.
                let next_start = pair_start + 2;
                let skip = (next_start - line_start) as usize;
                accum.drain(..skip);
                line_start = next_start;
            }

            if current_offset >= self.total_bytes {
                break;
            }
        }

        // Trailing partial line.
        if !accum.is_empty() && lines.len() < count {
            let line = decode_line(&accum, self.encoding);
            current_offset = line_start + accum.len() as u64;
            lines.push(line);
        }

        Ok((lines, current_offset))
    }

    fn resolve_byte_offset(&self, target: &SeekTarget) -> u64 {
        match target {
            SeekTarget::ByteOffset(offset) => (*offset).min(self.total_bytes),
            SeekTarget::Fraction(f) => {
                let f = f.clamp(0.0, 1.0);
                (f * self.total_bytes as f64) as u64
            }
            SeekTarget::Line(line_num) => {
                // ByteSeek doesn't have a line index, so estimate using average line length (80 chars)
                // This is a rough approximation but much better than returning 0
                let estimated_offset = (*line_num as u64) * 80;
                debug!(
                    "ByteSeekBackend: Line({}) requested, estimating byte offset {} (using avg 80 chars/line)",
                    line_num, estimated_offset
                );
                estimated_offset.min(self.total_bytes)
            }
        }
    }
}

impl FileViewerBackend for ByteSeekBackend {
    fn get_lines(&self, target: &SeekTarget, count: usize) -> Result<LineChunk, ViewerError> {
        let raw_offset = self.resolve_byte_offset(target);

        debug!(
            "ByteSeekBackend::get_lines: target={:?}, resolved to byte offset {}",
            target, raw_offset
        );

        // Find the actual line start by scanning backward
        let mut file = File::open(&self.path)?;
        let line_start = self.find_line_start(&mut file, raw_offset)?;

        debug!(
            "ByteSeekBackend::get_lines: line_start={} (after backward scan)",
            line_start
        );

        let (lines, _end_offset) = self.read_lines_from(line_start, count)?;

        // Estimate line number based on byte position and average line length
        // This is an approximation, but better than always returning 0
        let estimated_line_number = if line_start == 0 {
            0
        } else {
            // Use the lines we just read to estimate avg line length
            let avg_line_len = if !lines.is_empty() {
                let total_bytes_in_chunk: usize = lines.iter().map(|l| l.len() + 1).sum(); // +1 for newline
                total_bytes_in_chunk / lines.len()
            } else {
                80 // fallback assumption
            };
            (line_start as usize) / avg_line_len.max(1)
        };

        debug!(
            "ByteSeekBackend::get_lines: returning {} lines, estimated first_line_number={} (based on avg line len)",
            lines.len(),
            estimated_line_number
        );

        Ok(LineChunk {
            lines,
            // Estimate line number based on byte offset
            first_line_number: estimated_line_number,
            byte_offset: line_start,
            total_lines: None,
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
        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(0))?;

        let chunk_size: usize = 1024 * 1024; // 1 MB chunks
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

            // Combine leftover + new data into a working buffer
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
                    let cf = scan_line_with_matcher(matcher, &line, line_number, line_byte_offset, cancel, results);
                    match cf {
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
                    // Incomplete line, save as leftover for next iteration
                    leftover.extend_from_slice(&data[pos..]);
                    break;
                }
            }

            // Update progress after each chunk so the frontend can show real progress
            *progress.lock_ignore_poison() = scanned;
        }

        // Handle last line without newline (only reached if limit not hit; loop breaks early otherwise)
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
            supports_line_seek: false,
            supports_byte_seek: true,
            supports_fraction_seek: true,
            knows_total_lines: false,
        }
    }

    fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    fn total_lines(&self) -> Option<usize> {
        None
    }

    fn file_name(&self) -> &str {
        &self.file_name
    }
}

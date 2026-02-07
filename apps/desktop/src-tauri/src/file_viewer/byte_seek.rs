//! ByteSeekBackend — byte-offset seeking with no pre-scan.
//!
//! Opens the file and can immediately serve lines at any byte position.
//! Scans backward up to MAX_BACKWARD_SCAN bytes to find a newline boundary.
//! If no newline is found (for example, in a binary file), treats the seek position as a line start.
//!
//! Supports Fraction seeking by multiplying fraction × total_bytes.
//! Does NOT support Line seeking (use LineIndexBackend for that).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use log::debug;
use memchr::memchr;

use super::{
    BackendCapabilities, FileViewerBackend, LineChunk, MAX_BACKWARD_SCAN, SearchMatch, SeekTarget, ViewerError,
};

pub struct ByteSeekBackend {
    path: std::path::PathBuf,
    total_bytes: u64,
    file_name: String,
}

impl ByteSeekBackend {
    pub fn open(path: &Path) -> Result<Self, ViewerError> {
        let metadata = std::fs::metadata(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => ViewerError::NotFound(path.display().to_string()),
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
        })
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

        // Search backward for '\n' — the line starts right after the last newline before offset.
        // We search in reverse using memchr on the reversed slice approach.
        if let Some(pos) = buf.iter().rposition(|&b| b == b'\n') {
            // Line starts right after this newline
            Ok(scan_start + pos as u64 + 1)
        } else {
            // No newline found within MAX_BACKWARD_SCAN — treat scan_start as line start
            // (or if scan_start == 0, the file starts here)
            Ok(scan_start)
        }
    }

    /// Read `count` lines starting from `byte_offset`.
    /// Returns the lines and the byte offset just past the last line read.
    fn read_lines_from(&self, start_offset: u64, count: usize) -> Result<(Vec<String>, u64), ViewerError> {
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
                    let line = String::from_utf8_lossy(line_bytes).into_owned();
                    lines.push(line);
                    current_offset += (nl_pos + 1) as u64; // +1 for newline
                    pos += nl_pos + 1;
                } else {
                    // No newline in remaining data — save as leftover
                    leftover.extend_from_slice(&data[pos..]);
                    continue 'outer;
                }
            }
        }

        // If there's leftover data (last line without newline), add it
        if !leftover.is_empty() && lines.len() < count {
            let line = String::from_utf8_lossy(&leftover).into_owned();
            current_offset += leftover.len() as u64;
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

    fn search(&self, query: &str, cancel: &AtomicBool, results: &Mutex<Vec<SearchMatch>>) -> Result<u64, ViewerError> {
        let query_lower = query.to_lowercase();
        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(0))?;

        let chunk_size: usize = 1024 * 1024; // 1 MB chunks
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
                    // Incomplete line — save as leftover for next iteration
                    leftover.extend_from_slice(&data[pos..]);
                    break;
                }
            }
        }

        // Handle last line without newline
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

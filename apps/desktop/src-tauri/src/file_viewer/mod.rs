//! File viewer module — on-demand line serving with three backend strategies.
//!
//! Backends:
//! - `FullLoadBackend`: loads entire file into memory (small files, <1 MB)
//! - `LineIndexBackend`: sparse line-offset index, O(lines/256) memory
//! - `ByteSeekBackend`: byte-offset seeking, no pre-scan needed (instant open)

mod byte_seek;
mod full_load;
mod line_index;
mod session;

#[cfg(test)]
mod byte_seek_test;
#[cfg(test)]
mod full_load_test;
#[cfg(test)]
mod line_index_test;
#[cfg(test)]
mod session_test;

pub use session::{
    SearchPollResult, ViewerOpenResult, ViewerSessionStatus, close_session, get_lines, get_session_status,
    open_session, search_cancel, search_poll, search_start,
};

use serde::Serialize;

/// Maximum file size for FullLoadBackend (1 MB).
const FULL_LOAD_THRESHOLD: u64 = 1024 * 1024;

/// Interval between line index checkpoints (every 256 lines).
const INDEX_CHECKPOINT_INTERVAL: usize = 256;

/// Maximum bytes to scan backward when seeking by byte offset.
const MAX_BACKWARD_SCAN: usize = 8192;

/// Where to seek in the file.
#[derive(Debug, Clone)]
pub enum SeekTarget {
    /// Jump to a specific line number (0-based).
    Line(usize),
    /// Jump to a byte offset and find the surrounding line.
    ByteOffset(u64),
    /// Jump to a fraction of the file (0.0 = start, 1.0 = end).
    Fraction(f64),
}

/// A chunk of lines returned by a backend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineChunk {
    pub lines: Vec<String>,
    /// 0-based.
    pub first_line_number: usize,
    pub byte_offset: u64,
    /// Known only after full scan or full load.
    pub total_lines: Option<usize>,
    pub total_bytes: u64,
}

/// A search match found by a backend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchMatch {
    /// 0-based.
    pub line: usize,
    /// Byte offset within the line.
    pub column: usize,
    /// In bytes.
    pub length: usize,
}

/// What a backend can do.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendCapabilities {
    pub supports_line_seek: bool,
    pub supports_byte_seek: bool,
    pub supports_fraction_seek: bool,
    pub knows_total_lines: bool,
}

/// Errors from the viewer backends.
#[derive(Debug, Clone)]
pub enum ViewerError {
    Io(String),
    NotFound(String),
    IsDirectory,
    SessionNotFound(String),
}

impl std::fmt::Display for ViewerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(msg) => write!(f, "{}", msg),
            Self::NotFound(path) => write!(f, "File not found: {}", path),
            Self::IsDirectory => write!(f, "Cannot view a directory"),
            Self::SessionNotFound(id) => write!(f, "Viewer session not found: {}", id),
        }
    }
}

impl From<std::io::Error> for ViewerError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

/// The interface all viewer backends implement.
pub trait FileViewerBackend: Send + Sync {
    /// Fetch a range of lines starting from the given target.
    fn get_lines(&self, target: &SeekTarget, count: usize) -> Result<LineChunk, ViewerError>;

    /// Search for a query string, populating matches into the provided vec.
    /// Checks the cancel flag periodically and stops early if set.
    /// Returns the total number of bytes scanned (for progress reporting).
    fn search(
        &self,
        query: &str,
        cancel: &std::sync::atomic::AtomicBool,
        matches: &std::sync::Mutex<Vec<SearchMatch>>,
    ) -> Result<u64, ViewerError>;

    /// What this backend can do.
    fn capabilities(&self) -> BackendCapabilities;

    /// Total file size in bytes.
    fn total_bytes(&self) -> u64;

    /// Total lines if known (only FullLoad and completed LineIndex know this).
    fn total_lines(&self) -> Option<usize>;

    /// File name (last path component).
    fn file_name(&self) -> &str;
}

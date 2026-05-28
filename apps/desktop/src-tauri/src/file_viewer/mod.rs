//! File viewer module: on-demand line serving with three backend strategies.
//!
//! Backends:
//! - `FullLoadBackend`: loads entire file into memory (small files, <1 MB)
//! - `LineIndexBackend`: sparse line-offset index, O(lines/256) memory
//! - `ByteSeekBackend`: byte-offset seeking, no pre-scan needed (instant open)

mod byte_seek;
mod encoding;
mod full_load;
mod line_index;
mod range_read;
mod search_matcher;
mod session;

#[cfg(test)]
mod byte_seek_test;
#[cfg(test)]
mod encoding_test;
#[cfg(test)]
mod full_load_test;
#[cfg(test)]
mod line_index_test;
#[cfg(test)]
mod search_matcher_test;
#[cfg(test)]
mod session_test;

pub use encoding::FileEncoding;
pub use range_read::RangeEnd;
pub use search_matcher::{Matcher, SearchMode};
pub use session::{
    EncodingOptions, SearchPollResult, ViewerOpenResult, ViewerSessionStatus, cancel_read, close_session,
    get_encoding_options, get_lines, get_session_status, open_session, read_range, search_cancel, search_poll,
    search_start, set_encoding, write_range_to_file,
};

use serde::Serialize;

/// Maximum file size for FullLoadBackend (1 MB).
const FULL_LOAD_THRESHOLD: u64 = 1024 * 1024;

/// Interval between line index checkpoints (every 256 lines).
const INDEX_CHECKPOINT_INTERVAL: usize = 256;

/// Maximum bytes to scan backward when seeking by byte offset.
const MAX_BACKWARD_SCAN: usize = 8192;

/// Maximum number of matches stored during search. Once reached, the search stops entirely.
/// The frontend highlights additional matches client-side on visible lines, so stopping early
/// doesn't lose highlighting: it only caps the prev/next navigation index.
const MAX_SEARCH_MATCHES: usize = 10_000;

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
#[derive(Debug, Clone, Serialize, specta::Type)]
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
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchMatch {
    /// 0-based.
    pub line: usize,
    /// UTF-16 code unit offset within the line (matches JS string indexing).
    pub column: usize,
    /// Length in UTF-16 code units (matches JS string indexing).
    pub length: usize,
    /// Byte offset of the start of the line containing this match.
    /// Used by the frontend to scroll accurately in ByteSeek mode where line numbers
    /// don't map to the virtual scroll coordinate system.
    pub byte_offset: u64,
}

/// What a backend can do.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BackendCapabilities {
    pub supports_line_seek: bool,
    pub supports_byte_seek: bool,
    pub supports_fraction_seek: bool,
    pub knows_total_lines: bool,
}

/// Errors from the viewer backends.
///
/// Variants carry the typed reason; the IPC layer maps these to user-facing strings.
/// The frontend matches on the variant tag (via `specta::Type`-generated bindings),
/// per the no-string-classification rule in AGENTS.md.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ViewerError {
    Io {
        message: String,
    },
    NotFound {
        path: String,
    },
    IsDirectory,
    SessionNotFound {
        session_id: String,
    },
    /// The read was cancelled via `viewer_cancel_read` (or session close).
    Cancelled,
    /// A requested line is past the file's last line.
    OutOfRange,
    /// The read exceeded the IPC timeout. The frontend can offer Retry; the underlying
    /// backend read continues until it sees the per-read cancel flag or completes.
    TimedOut,
}

impl std::fmt::Display for ViewerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { message } => write!(f, "{}", message),
            Self::NotFound { path } => write!(f, "File not found: {}", path),
            Self::IsDirectory => write!(f, "Cannot view a directory"),
            Self::SessionNotFound { session_id } => write!(f, "Viewer session not found: {}", session_id),
            Self::Cancelled => write!(f, "Read cancelled"),
            Self::OutOfRange => write!(f, "Selection is past the end of the file"),
            Self::TimedOut => write!(f, "Read timed out"),
        }
    }
}

impl From<std::io::Error> for ViewerError {
    fn from(e: std::io::Error) -> Self {
        Self::Io { message: e.to_string() }
    }
}

/// The interface all viewer backends implement.
pub trait FileViewerBackend: Send + Sync {
    /// Fetch a range of lines starting from the given target.
    fn get_lines(&self, target: &SeekTarget, count: usize) -> Result<LineChunk, ViewerError>;

    /// Search the file with the given `Matcher`, populating matches into the provided vec.
    /// Checks the cancel flag at chunk, line, and match granularity and stops early if set.
    /// Updates `progress` with the number of bytes scanned so far.
    /// Returns the total number of bytes scanned.
    fn search(
        &self,
        matcher: &Matcher,
        cancel: &std::sync::atomic::AtomicBool,
        matches: &std::sync::Mutex<Vec<SearchMatch>>,
        progress: &std::sync::Mutex<u64>,
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

//! File viewer module: on-demand line serving with three backend strategies.
//!
//! Backends:
//! - `FullLoadBackend`: loads entire file into memory (small files, <1 MB)
//! - `LineIndexBackend`: sparse line-offset index, O(lines/256) memory
//! - `ByteSeekBackend`: byte-offset seeking, no pre-scan needed (instant open)

mod archive_extract;
mod byte_seek;
pub mod content_kind;
pub mod encoding;
mod full_load;
mod line_index;
pub mod media;
mod media_backend;
pub mod media_protocol;
mod media_session;
mod range_read;
mod search_matcher;
pub mod session;
pub mod watcher;

#[cfg(test)]
mod archive_extract_test;
#[cfg(test)]
mod byte_seek_test;
#[cfg(test)]
mod content_kind_test;
#[cfg(test)]
mod encoding_test;
#[cfg(test)]
mod full_load_test;
#[cfg(test)]
mod line_index_test;
#[cfg(test)]
mod media_protocol_test;
#[cfg(test)]
mod media_session_test;
#[cfg(test)]
mod search_matcher_test;
#[cfg(test)]
mod session_test;
#[cfg(test)]
mod watcher_test;

pub use archive_extract::init_archive_extract_dir;
pub use content_kind::{ViewerContentKind, classify_viewer_content};
pub use encoding::FileEncoding;
pub use media_session::MediaDimensions;
pub use range_read::RangeEnd;
pub use search_matcher::{Matcher, SearchMode};
pub use session::{
    EncodingOptions, SearchPollResult, ViewerOpenResult, ViewerSessionStatus, cancel_read, close_session,
    close_session_for_window, get_encoding_options, get_lines, get_session_status, init_app_handle, open_session,
    open_session_as_text, read_range, register_window_session, reload, search_cancel, search_poll, search_start,
    set_encoding, set_tail_mode, write_range_to_file,
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
    /// Previewing a file inside an archive would extract more than the preview cap.
    /// Refused before any extraction (the zip-bomb guard for preview); `size` is the
    /// entry's declared uncompressed size, `cap` the limit. See
    /// `file_viewer::archive_extract`.
    ExtractTooLarge {
        size: u64,
        cap: u64,
    },
    /// Saving a selection to a destination INSIDE an archive isn't supported (archives
    /// are read-only in this phase). Rejected by `viewer_write_range_to_file`.
    DestinationInsideArchive,
    /// The archive entry can't be previewed (encrypted, corrupt, or an unsupported
    /// codec). Carries a message; the FE renders it without inspecting the string.
    Archive {
        message: String,
    },
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
            Self::ExtractTooLarge { size, cap } => {
                write!(
                    f,
                    "This item is too large to preview from the archive ({size} bytes, limit {cap})"
                )
            }
            Self::DestinationInsideArchive => write!(f, "Can't save into an archive"),
            Self::Archive { message } => write!(f, "{message}"),
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

    /// Returns a fresh boxed backend whose internal state covers bytes up to
    /// `new_size`. Cancellable. Default is `Err(ViewerError::Cancelled)` so
    /// backends that don't support extension (today's `FullLoadBackend`) cause
    /// the session to escalate to a different backend instead.
    ///
    /// Concrete impls override; the trait-level default panics rather than
    /// silently dropping the append.
    fn extend_to_boxed(
        &self,
        _new_size: u64,
        _cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<Box<dyn FileViewerBackend>, ViewerError> {
        Err(ViewerError::Io {
            message: "backend does not support extend_to".to_string(),
        })
    }

    /// Returns a fresh boxed backend whose internal state is identical to
    /// `self` but with the encoding field swapped to `new_encoding`. Used by
    /// the `set_encoding` instant-swap path when `same_byte_layout` holds: the
    /// existing newline index is still valid under the new encoding, so only
    /// the decoder needs to change. Default is `None`, meaning the session
    /// must take the slow rebuild path.
    fn with_encoding(&self, _new_encoding: FileEncoding) -> Option<Box<dyn FileViewerBackend>> {
        None
    }

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

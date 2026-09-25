//! File viewer module: on-demand line serving with three backend strategies.
//!
//! Backends:
//! - `FullLoadBackend`: loads entire file into memory (small files, <1 MB)
//! - `LineIndexBackend`: sparse line-offset index, O(lines/256) memory
//! - `ByteSeekBackend`: byte-offset seeking, no pre-scan needed (instant open)

pub(crate) mod analytics;
mod byte_seek;
pub mod content_kind;
pub mod encoding;
mod full_load;
pub(crate) mod headless;
mod line_index;
pub(crate) mod materialize;
pub mod media;
mod media_backend;
pub mod media_protocol;
mod media_session;
pub mod pending_open;
pub(crate) mod range_read;
mod row_walk;
mod rows;
mod search_matcher;
pub mod session;
pub mod watcher;

#[cfg(test)]
mod analytics_test;
#[cfg(test)]
mod byte_seek_test;
#[cfg(test)]
mod content_kind_test;
#[cfg(test)]
mod encoding_test;
#[cfg(test)]
mod full_load_test;
#[cfg(test)]
mod headless_test;
#[cfg(test)]
mod line_index_test;
#[cfg(test)]
mod materialize_test;
#[cfg(test)]
mod media_protocol_test;
#[cfg(test)]
mod media_session_test;
#[cfg(test)]
mod row_characterization_test;
#[cfg(test)]
mod rows_test;
#[cfg(test)]
mod search_cancel_test_support;
#[cfg(test)]
mod search_matcher_test;
#[cfg(test)]
mod session_test;
#[cfg(test)]
mod watcher_test;

pub use content_kind::{ViewerContentKind, classify_viewer_content};
pub use encoding::FileEncoding;
pub use materialize::init_materialize_dir;
pub use media_session::MediaDimensions;
pub use pending_open::{AbandonReason, PendingOpen, ViewerPullProgress, begin_pending_open, end_pending_open};
pub use range_read::RangeEnd;
pub use row_walk::{CHUNK_BUDGET_BYTES, ChunkEnd, TotalRows, ViewerRow};
pub use rows::SEGMENT_BYTES;
pub use search_matcher::{Matcher, SearchMode};
pub use session::{
    EncodingOptions, SaveProgress, SearchPollResult, ViewerOpenResult, ViewerSessionStatus, cancel_read, close_session,
    close_session_for_window, get_bytes, get_encoding_options, get_lines, get_session_status, init_app_handle,
    open_for_window, open_session, open_session_as_text, read_range, register_window_session, reload, search_cancel,
    search_poll, search_start, set_encoding, set_tail_mode, write_range_to_file,
};

use serde::{Deserialize, Serialize};

/// Maximum file size for FullLoadBackend (1 MB).
const FULL_LOAD_THRESHOLD: u64 = 1024 * 1024;

/// Interval between line index checkpoints, in ROWS.
///
/// ❗ Rows, not lines: a file with no newline in it has ONE line, so a line-counted
/// interval gives it a single checkpoint and every fetch rescans from byte 0. That
/// breaks invariant I1 on precisely the file rows exist for. Each checkpoint carries
/// the physical line number alongside, so the gutter still gets exact numbers from the
/// same single scan.
const INDEX_CHECKPOINT_INTERVAL: usize = 256;

/// Maximum number of matches stored during search. Once reached, the search stops entirely.
/// The frontend highlights additional matches client-side on visible lines, so stopping early
/// doesn't lose highlighting: it only caps the prev/next navigation index.
pub(crate) const MAX_SEARCH_MATCHES: usize = 10_000;

/// Which kind of seek the caller is asking for, as a value the wire enforces.
///
/// The command pairs it with a numeric `target_value`; keeping the KIND an enum
/// is what stops the backend re-parsing a free-form string and inventing an
/// error arm for a case typed callers can't reach.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SeekTargetKind {
    /// `target_value` is a 0-based line number.
    Line,
    /// `target_value` is a byte offset.
    Byte,
    /// `target_value` is a fraction of the file (0.0 = start, 1.0 = end).
    Fraction,
}

/// Where to seek in the file.
///
/// The numeric coordinate is a ROW index, not a physical line; renaming the variant
/// to match is open (GitHub #263).
#[derive(Debug, Clone)]
pub enum SeekTarget {
    /// Jump to a specific row (0-based). Exact on `FullLoadBackend` and
    /// `LineIndexBackend`; `ByteSeekBackend` maps it through its bytes-per-row sample.
    Line(usize),
    /// Jump to a byte offset and find the row containing it.
    ByteOffset(u64),
    /// Jump to a fraction of the file (0.0 = start, 1.0 = end).
    Fraction(f64),
}

/// A chunk of ROWS returned by a backend.
///
/// A row ends at a newline or after `SEGMENT_BYTES`, whichever comes first
/// (`file_viewer::rows`), so one fetch costs the same on a 50 GB single-line file as on
/// an ordinary one. Each row says whether the break at its end is the file's or Cmdr's.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LineChunk {
    pub rows: Vec<ViewerRow>,
    /// 0-based row index of the first row. An estimate on `ByteSeekBackend`.
    pub first_row_number: usize,
    /// Absolute offset of the first row's first byte.
    ///
    /// ❗ The row's OWN offset. Returning an index checkpoint's instead is what made
    /// every onward seek land short, duplicating a line at each chunk seam.
    pub byte_offset: u64,
    /// Absolute offset just past the last row served, from the SOURCE bytes.
    ///
    /// ❗ A caller fetching the next chunk steers by this. ❌ Never re-derive it by
    /// summing decoded string lengths: those are UTF-8 even when the file is UTF-16,
    /// and they carry no newline.
    pub end_byte_offset: u64,
    /// Whether the chunk ran out of rows, out of budget, or out of file.
    pub end: ChunkEnd,
    pub total_rows: TotalRows,
    pub total_bytes: u64,
}

#[cfg(test)]
impl LineChunk {
    /// Just the rows' text.
    ///
    /// Test convenience only: production reads `rows`, because a row's `continues` flag
    /// is what stops a copy path joining two of them with a newline the file never had.
    pub fn texts(&self) -> Vec<String> {
        self.rows.iter().map(|row| row.text.clone()).collect()
    }
}

/// A search match found by a backend.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchMatch {
    /// 0-based ROW index (the coordinate is already a row; the field rename is open,
    /// GitHub #263). Search scans rows, so a match inside a 300 MB line comes back with a
    /// column that fits on screen instead of one 2.5 million units wide.
    pub line: usize,
    /// UTF-16 code unit offset within the ROW (matches JS string indexing). Bounded by
    /// the row's length, which is bounded by two segments.
    pub column: usize,
    /// Length in UTF-16 code units (matches JS string indexing).
    pub length: usize,
    /// Byte offset of the start of the row containing this match.
    /// Used by the frontend to scroll accurately in ByteSeek mode where row numbers
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
    /// A pull into the preview temp got no bytes for the stall limit: the phone or
    /// server went quiet. The frontend offers Retry; the pull stops at its next chunk
    /// boundary and removes its temp. See `file_viewer::pending_open`.
    StoppedResponding,
    /// Previewing a file the viewer has to pull into a temp first (an archive entry,
    /// a file in a repo's `.git` snapshot, a file on a phone or server) would
    /// materialize more than the preview cap. Refused before any extraction (the
    /// zip-bomb guard for preview); `size` is the file's reported size, `cap` the
    /// limit. See `file_viewer::materialize`.
    TooLargeToPreview {
        size: u64,
        cap: u64,
    },
    /// Saving a selection to a destination a ROUTE serves isn't supported: inside a
    /// `.zip`, or inside a repo's virtual `.git` trees. Neither has a directory on
    /// disk for the write to land in, and both are read-only besides. Rejected by
    /// `viewer_write_range_to_file`.
    DestinationIsReadOnly,
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
            Self::StoppedResponding => write!(f, "The source stopped sending the file"),
            Self::TooLargeToPreview { size, cap } => {
                // Display/log string only — the user sees the FE's friendly copy
                // (`viewer.error.tooLargeToPreview`). Phrased to avoid a `1 bytes`
                // singular. Names no namespace: a `.zip` entry and a `.git` snapshot
                // blob both reach this cap.
                write!(
                    f,
                    "This item is too large to preview from here (size {size}, limit {cap})"
                )
            }
            // Display/log string only — the user sees the FE's friendly copy
            // (`viewer.saveAs.destinationReadOnly`). Names no namespace, because
            // both a `.zip` and a `.git` snapshot reach it.
            Self::DestinationIsReadOnly => write!(f, "Can't save into a read-only location"),
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

    /// How many ROWS the file has, and whether that is a count or an estimate. Every
    /// backend can answer: `ByteSeekBackend` divides by the bytes-per-row it sampled at
    /// open instead of returning nothing.
    fn total_rows(&self) -> TotalRows;

    /// Total lines if known (only FullLoad and completed LineIndex know this).
    fn total_lines(&self) -> Option<usize>;

    /// File name (last path component).
    fn file_name(&self) -> &str;
}

//! Opening a text backend with no session around it.
//!
//! The viewer's `open_session` wraps a backend in a `ViewerSession`: a `SESSIONS` entry,
//! a watcher, a media token, a background ByteSeek→LineIndex upgrade thread. A caller
//! that wants to read a file the way the viewer reads it, but hand nothing to a window
//! (the Ask Cmdr `inspect_file` tool), needs only the backend. This is that seam: pick
//! the backend by the viewer's own size rule, honour the caller's cancel flag, and
//! return an immutable value the caller drops when done.
//!
//! Kept free of session concerns on purpose, so `open_session_core` could ride it too if
//! it were ever split.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use super::byte_seek::ByteSeekBackend;
use super::encoding::FileEncoding;
use super::full_load::FullLoadBackend;
use super::line_index::LineIndexBackend;
use super::{FULL_LOAD_THRESHOLD, FileViewerBackend, ViewerError};

/// A text backend plus the one fact a reader must relay: whether the line numbers it
/// reports are exact.
pub(crate) struct HeadlessBackend {
    pub backend: Box<dyn FileViewerBackend>,
    /// `false` only on the ByteSeek fallback, where `SeekTarget::Line(n)` resolves by an
    /// 80-bytes-a-line estimate and `total_lines` is unknown.
    pub line_numbers_exact: bool,
}

/// Open the backend the viewer would end up with for `path`, synchronously.
///
/// - Up to [`FULL_LOAD_THRESHOLD`]: `FullLoadBackend` (the whole file in memory, exact
///   lines).
/// - Larger: `LineIndexBackend`, scanning the file under `cancel` (about 2 s per GB on an
///   SSD; the viewer runs this same scan as its background upgrade). When the scan sees
///   `cancel` flipped it returns `Cancelled`, and this falls back to `ByteSeekBackend`
///   with `line_numbers_exact = false` rather than failing: an approximate window beats
///   no window, as long as the caller says it's approximate.
///
/// `encoding` comes from the caller (`encoding::detect_from_head` on the head it already
/// read), so the file isn't sniffed twice. Any other error is the backend's own typed
/// `ViewerError`.
pub(crate) fn open_text_backend(
    path: &Path,
    encoding: FileEncoding,
    cancel: &AtomicBool,
) -> Result<HeadlessBackend, ViewerError> {
    let size = std::fs::metadata(path)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => ViewerError::NotFound {
                path: path.display().to_string(),
            },
            _ => ViewerError::from(e),
        })?
        .len();
    if size <= FULL_LOAD_THRESHOLD {
        return Ok(HeadlessBackend {
            backend: Box::new(FullLoadBackend::open_with_encoding(path, encoding)?),
            line_numbers_exact: true,
        });
    }
    match LineIndexBackend::open_with_encoding(path, encoding, cancel) {
        Ok(indexed) => Ok(HeadlessBackend {
            backend: Box::new(indexed),
            line_numbers_exact: true,
        }),
        Err(ViewerError::Cancelled) => Ok(HeadlessBackend {
            backend: Box::new(ByteSeekBackend::open_with_encoding(path, encoding)?),
            line_numbers_exact: false,
        }),
        Err(other) => Err(other),
    }
}

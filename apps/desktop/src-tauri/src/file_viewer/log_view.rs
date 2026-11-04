//! The debug log's view of a text session: every row shows a marked value as itself, its log
//! markers gone (the host's `log_display`, the app's `redact::display_line`), while the file on disk keeps them for the error
//! reporter. The view applies where rows leave a backend, so search columns, highlights, wrap,
//! copy, and "save range" all work on the text the user sees; byte offsets stay the raw file's.

use super::row_text::RowView;
use super::search_matcher::Matcher;
use super::{
    BackendCapabilities, FileEncoding, FileViewerBackend, LineChunk, SearchMatch, SeekTarget, TotalRows, ViewerError,
};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

/// A backend whose rows come out through a [`RowView`]. The session wraps its current backend
/// on every read (`ViewerSession::load_backend`), so a backend swap (the index upgrade, an
/// encoding switch, a tail extend) keeps the view without any swap site knowing about it.
pub(super) struct ViewedBackend {
    pub(super) inner: Arc<Box<dyn FileViewerBackend>>,
    pub(super) rows: RowView,
}

impl FileViewerBackend for ViewedBackend {
    fn get_lines(&self, target: &SeekTarget, count: usize, cancel: &AtomicBool) -> Result<LineChunk, ViewerError> {
        let mut chunk = self.inner.get_lines(target, count, cancel)?;
        self.rows.apply(&mut chunk);
        Ok(chunk)
    }

    fn extend_to_boxed(&self, new_size: u64, cancel: &AtomicBool) -> Result<Box<dyn FileViewerBackend>, ViewerError> {
        self.inner.extend_to_boxed(new_size, cancel)
    }

    fn with_encoding(&self, new_encoding: FileEncoding) -> Option<Box<dyn FileViewerBackend>> {
        self.inner.with_encoding(new_encoding)
    }

    /// The matcher already reads rows the way the session shows them (`search_start` gives it
    /// the session's view), so the wrapped backend's walk serves the search as is.
    fn search(
        &self,
        matcher: &Matcher,
        cancel: &AtomicBool,
        matches: &Mutex<Vec<SearchMatch>>,
        progress: &Mutex<u64>,
    ) -> Result<u64, ViewerError> {
        self.inner.search(matcher, cancel, matches, progress)
    }

    fn capabilities(&self) -> BackendCapabilities {
        self.inner.capabilities()
    }

    fn total_bytes(&self) -> u64 {
        self.inner.total_bytes()
    }

    fn total_rows(&self) -> TotalRows {
        self.inner.total_rows()
    }

    fn total_lines(&self) -> Option<usize> {
        self.inner.total_lines()
    }

    fn file_name(&self) -> &str {
        self.inner.file_name()
    }
}

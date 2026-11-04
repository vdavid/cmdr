//! The `viewer_opened` facts, for the host's usage stats. ❌ Never a file name or
//! extension: the app's `file_viewer/analytics.rs` turns these into coarse tokens.

use super::super::{ViewerError, ViewerOpenResult};

/// Hears every open, once, however it ended.
pub trait ViewerAnalytics: Send + Sync {
    /// An open ended with `result`. `from_archive` says the path sat inside an archive;
    /// `force_text` is the "View as text" override.
    fn viewer_opened(&self, result: &Result<ViewerOpenResult, ViewerError>, from_archive: bool, force_text: bool);
}

/// Counts nothing.
pub struct NoAnalytics;

impl ViewerAnalytics for NoAnalytics {
    fn viewer_opened(&self, _result: &Result<ViewerOpenResult, ViewerError>, _from_archive: bool, _force_text: bool) {}
}

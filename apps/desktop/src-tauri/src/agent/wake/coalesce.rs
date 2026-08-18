//! The coalescer.

use std::time::Duration;

use super::{EventBundle, FolderEvent};

/// Fold events into per-folder counters, one bundle per folder per window.
pub fn coalesce(_events: &[FolderEvent], _window: Duration) -> Vec<EventBundle> {
    Vec::new()
}

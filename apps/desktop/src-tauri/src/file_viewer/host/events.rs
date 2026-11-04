//! What the viewer tells its window, as facts. The host owns the wire: the app turns each
//! into the Tauri event the frontend listens for (`file_viewer/app_host.rs`), so no wire
//! type lives on this side.

/// How a watched file changed under its session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChange {
    /// The file grew to `new_size` bytes (tail mode follows it).
    Grew { new_size: u64 },
    /// The file was replaced or truncated; the session reopened it.
    Rotated,
}

/// Where the viewer reports what happened. Called from background jobs, never under a
/// session lock; an implementation forwards and returns.
pub trait ViewerEvents: Send + Sync {
    /// The file `session_id` shows changed on disk.
    fn file_changed(&self, session_id: &str, change: FileChange);
}

/// Drops every report.
pub struct NoEvents;

impl ViewerEvents for NoEvents {
    fn file_changed(&self, _session_id: &str, _change: FileChange) {}
}

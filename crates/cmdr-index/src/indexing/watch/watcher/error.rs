//! [`WatcherError`], why a drive watcher didn't start, and the one text a log line marks its
//! root in.

use std::path::PathBuf;

use cmdr_fs::log_error::{ErrorLogText, ErrorText, write_plain};
use cmdr_fs::log_private::LogPrivate;

/// Errors that can occur when starting the watcher.
#[derive(Debug)]
pub enum WatcherError {
    /// Used on macOS (FSEvents); not constructed on Linux.
    #[allow(dead_code, reason = "constructed on macOS, not Linux")]
    Io(std::io::Error),
    /// Used on Linux (inotify) and stub platforms; not constructed on macOS.
    #[allow(dead_code, reason = "constructed on Linux/stub, not macOS")]
    StreamCreate(String),
    /// inotify refused a watch on one root (it vanished, or `max_user_watches` ran out). The
    /// root stays typed so a log line marks it. Linux only.
    #[allow(dead_code, reason = "constructed on Linux, not macOS")]
    WatchRefused {
        /// The folder the watch was asked for.
        root: PathBuf,
        /// What `notify` said, without the paths it appends.
        cause: String,
    },
}

impl std::fmt::Display for WatcherError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write_plain(self, f)
    }
}

/// The one text behind `Display` and `LogError`: a refused watch marks its root.
impl ErrorLogText for WatcherError {
    fn write_log_text(&self, out: &mut ErrorText<'_>) -> std::fmt::Result {
        use std::fmt::Write as _;
        match self {
            WatcherError::Io(e) => write!(out, "I/O error: {e}"),
            WatcherError::StreamCreate(msg) => write!(out, "Failed to create watcher: {msg}"),
            WatcherError::WatchRefused { root, cause } => {
                out.write_str("Failed to create watcher: inotify watch on ")?;
                out.value(LogPrivate::dir_path(root))?;
                write!(out, ": {cause}")
            }
        }
    }
}

impl std::error::Error for WatcherError {}

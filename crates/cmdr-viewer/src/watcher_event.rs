//! What a tailed file did, as the session reacts to it. Its own module so sessions build
//! where the watcher (`watcher.rs`, native only) doesn't.

/// Classified file-system event for a tailed file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    target_family = "wasm",
    allow(dead_code, reason = "only the native watcher raises these")
)]
pub enum WatcherEvent {
    /// File size grew to `new_size` bytes.
    Grew(u64),
    /// File was truncated or shrunk.
    Shrunk,
    /// File was replaced (rename, atomic-write swap, inode change).
    Replaced,
    /// Metadata changed but the byte content did not (mtime, permissions).
    MetadataOnly,
}

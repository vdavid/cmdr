//! The byte-fetch seam for network enrichment: read one image's compressed bytes off
//! an opted-in network volume, bounded against an indefinitely-blocking mount.
//!
//! ## Byte-fetch decision (plan Decision 6, network enrichment)
//!
//! Media enrichment MUST read image bytes off the wire — unlike `importance/`, which
//! never does. There is no sibling to copy, so this reuses the ONE byte-read path
//! Cmdr already has for images over SMB: the file viewer's `cmdr-media://` handler
//! reads SMB image bytes via the **OS mount path** (`/Volumes/<share>/…`) with plain
//! `std::fs` + a timeout (`file_viewer/media_protocol.rs`). We do the same: map the
//! index-relative path to its OS-mount-absolute form and `std::fs::read` it. We do NOT
//! stand up a parallel direct-`smb2` client (`Volume::open_read_stream`) — that's the
//! chunked large-transfer/copy path; an OCR fetch wants the whole (bounded) compressed
//! file, and matching the viewer keeps one transport for image bytes.
//!
//! **Non-blocking discipline.** A network `std::fs::read` can block indefinitely on a
//! dead/hung mount. So the read runs on a throwaway thread and the caller waits with a
//! timeout ([`FsByteFetcher`]); a timeout returns [`FetchError::Disconnected`] rather
//! than wedging the pass. Critically, the fetch happens in the enrich layer — NOT on
//! the serialized Vision OCR worker thread — so a hung mount can never stall OCR of
//! other (local) volumes.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// A hard cap on a single image's compressed bytes. Comfortably above any real photo
/// or RAW; a file past it is skipped (not read into memory) rather than risking an
/// OOM on a pathological input.
pub const MAX_FETCH_BYTES: u64 = 256 * 1024 * 1024;

/// Why a byte fetch didn't yield bytes. Typed (never string-matched — `no-string-
/// matching`): the caller branches on the variant to decide pause-vs-skip-vs-fail.
#[derive(Debug)]
pub enum FetchError {
    /// The mount is gone or hung (a timeout, or a non-`NotFound` I/O error). NOT a bad
    /// file — the caller PAUSES the volume (keeps rows, no GC, no `Failed`).
    Disconnected(String),
    /// The file no longer exists at the mount. A vanished source — the caller skips it
    /// (a completed scan's GC collects it), never marks it `Failed`.
    NotFound,
    /// The file is larger than [`MAX_FETCH_BYTES`]; skipped without reading.
    TooLarge,
}

/// Reads one image's compressed bytes for enrichment. Behind a trait so the enrich
/// core is testable with a scripted fake (no real mount, no I/O).
pub trait ByteFetcher: Send + Sync {
    /// Read the bytes at `os_path`, giving up after `timeout`.
    fn fetch(&self, os_path: &str, timeout: Duration) -> Result<Vec<u8>, FetchError>;
}

/// The production fetcher: `std::fs::read` on the OS mount path, on a throwaway thread
/// bounded by a timeout so a hung mount can't block the pass.
pub struct FsByteFetcher;

impl ByteFetcher for FsByteFetcher {
    fn fetch(&self, os_path: &str, timeout: Duration) -> Result<Vec<u8>, FetchError> {
        let (tx, rx) = mpsc::channel();
        let path = os_path.to_string();
        // A detached reader thread: if the mount hangs, we abandon it on timeout rather
        // than blocking. At most one lingering thread per hung mount (we pause on the
        // first timeout), which self-clears when the mount recovers or the app exits.
        thread::Builder::new()
            .name("media-net-fetch".into())
            .spawn(move || {
                let _ = tx.send(read_bounded(&path));
            })
            .map_err(|e| FetchError::Disconnected(format!("spawn fetch thread: {e}")))?;

        match rx.recv_timeout(timeout) {
            Ok(result) => result,
            // A timeout is the hung-mount signal: treat as disconnected (pause), not a
            // bad file.
            Err(mpsc::RecvTimeoutError::Timeout) => {
                Err(FetchError::Disconnected(format!("read timed out after {timeout:?}")))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(FetchError::Disconnected("fetch thread dropped".to_string()))
            }
        }
    }
}

/// Read a file, capping at [`MAX_FETCH_BYTES`] and classifying failures by I/O error
/// KIND (a typed errno, not a message match).
fn read_bounded(path: &str) -> Result<Vec<u8>, FetchError> {
    use std::io::Read;
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(FetchError::NotFound),
        Err(e) => return Err(FetchError::Disconnected(format!("open '{path}': {e}"))),
    };
    // Read at most MAX_FETCH_BYTES + 1 so we can tell "exactly at the cap" from "over".
    let mut buf = Vec::new();
    let mut limited = file.take(MAX_FETCH_BYTES + 1);
    match limited.read_to_end(&mut buf) {
        Ok(_) => {
            if buf.len() as u64 > MAX_FETCH_BYTES {
                Err(FetchError::TooLarge)
            } else {
                Ok(buf)
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(FetchError::NotFound),
        Err(e) => Err(FetchError::Disconnected(format!("read '{path}': {e}"))),
    }
}

/// Map a volume's index-relative path (`/DCIM/x.jpg`, reconstructed from the index's
/// `ROOT_ID`) to its OS-mount-absolute path by prepending the mount root. For the
/// `root`/local volume the mount root is `/`, so the index-relative path is already
/// absolute and passes through unchanged.
pub fn os_join(mount_root: &str, index_relative: &str) -> String {
    if mount_root == "/" || mount_root.is_empty() {
        return index_relative.to_string();
    }
    let trimmed = mount_root.trim_end_matches('/');
    // `index_relative` starts with '/', so this yields `<mount>/…` with no double slash.
    format!("{trimmed}{index_relative}")
}

/// A scripted fetcher for tests: maps an OS path to bytes, or to a disconnect, so the
/// enrich core's pause/resume paths run with no real mount.
#[cfg(test)]
pub struct FakeByteFetcher {
    bytes: std::collections::HashMap<String, Vec<u8>>,
    disconnected: std::collections::HashSet<String>,
}

#[cfg(test)]
impl Default for FakeByteFetcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl FakeByteFetcher {
    pub fn new() -> Self {
        Self {
            bytes: std::collections::HashMap::new(),
            disconnected: std::collections::HashSet::new(),
        }
    }

    /// Script bytes for an OS path.
    pub fn with_bytes(mut self, os_path: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        self.bytes.insert(os_path.into(), bytes.into());
        self
    }

    /// Script a disconnect (unmount) for an OS path.
    pub fn disconnect_on(mut self, os_path: impl Into<String>) -> Self {
        self.disconnected.insert(os_path.into());
        self
    }
}

#[cfg(test)]
impl ByteFetcher for FakeByteFetcher {
    fn fetch(&self, os_path: &str, _timeout: Duration) -> Result<Vec<u8>, FetchError> {
        if self.disconnected.contains(os_path) {
            return Err(FetchError::Disconnected("scripted unmount".to_string()));
        }
        match self.bytes.get(os_path) {
            Some(b) => Ok(b.clone()),
            None => Err(FetchError::NotFound),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_join_prepends_the_mount_root() {
        assert_eq!(os_join("/Volumes/naspi", "/DCIM/x.jpg"), "/Volumes/naspi/DCIM/x.jpg");
        assert_eq!(os_join("/Volumes/naspi/", "/DCIM/x.jpg"), "/Volumes/naspi/DCIM/x.jpg");
        // Root / local volume: index-relative is already absolute.
        assert_eq!(os_join("/", "/a/b.jpg"), "/a/b.jpg");
        assert_eq!(os_join("", "/a/b.jpg"), "/a/b.jpg");
    }

    #[test]
    fn fs_fetch_reads_a_real_file() {
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join("x.bin");
        std::fs::write(&path, b"hello bytes").expect("write");
        let bytes = FsByteFetcher
            .fetch(&path.to_string_lossy(), Duration::from_secs(5))
            .expect("fetch");
        assert_eq!(bytes, b"hello bytes");
    }

    #[test]
    fn fs_fetch_missing_file_is_not_found() {
        let err = FsByteFetcher
            .fetch("/nonexistent/cmdr/media/x.jpg", Duration::from_secs(5))
            .expect_err("missing file errors");
        assert!(matches!(err, FetchError::NotFound));
    }
}

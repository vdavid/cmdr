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
    /// The file exists but its BYTES couldn't be read (permission denied, a corrupt
    /// region, an exotic per-file refusal) — a per-file fault, NOT a transport one.
    /// The caller skips it, counts it, and logs the total at pass end; it never
    /// pauses the pass (that's reserved for a typed disconnect) and never marks the
    /// row `Failed` (which is for a good read with a bad decode). The M1 hazard this
    /// variant closes: a TCC `EPERM` on the OS mount classifying as "disconnected"
    /// paused the whole NAS pass after zero images.
    Unreadable(String),
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
/// KIND (a typed errno, not a message match — see [`classify_io_error`]).
fn read_bounded(path: &str) -> Result<Vec<u8>, FetchError> {
    use std::io::Read;
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => return Err(classify_io_error(&e, path, "open")),
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
        Err(e) => Err(classify_io_error(&e, path, "read")),
    }
}

/// Classify an OS-mount I/O failure by its typed errno (never a message match):
/// `NotFound` for a vanished source, [`FetchError::Disconnected`] ONLY for
/// transport-loss errnos (the mount is gone), and everything else — permission
/// denied, `EIO` on a corrupt region, `EISDIR` — as a per-file
/// [`FetchError::Unreadable`]. The bias is deliberate: misreading a dead mount as
/// per-file skips burns through the list and completes honestly (re-enriched on the
/// next scan), while misreading a per-file fault as "disconnected" pauses the whole
/// pass against a condition that never clears — the M1 TCC-EPERM stall.
fn classify_io_error(e: &std::io::Error, path: &str, op: &str) -> FetchError {
    if e.kind() == std::io::ErrorKind::NotFound {
        return FetchError::NotFound;
    }
    let transport_loss = matches!(
        e.raw_os_error(),
        Some(
            libc::ETIMEDOUT
                | libc::ENOTCONN
                | libc::ENETDOWN
                | libc::ENETUNREACH
                | libc::EHOSTDOWN
                | libc::EHOSTUNREACH
                | libc::ENODEV
                | libc::ENXIO
                | libc::ESTALE
        )
    );
    if transport_loss {
        FetchError::Disconnected(format!("{op} '{path}': {e}"))
    } else {
        FetchError::Unreadable(format!("{op} '{path}': {e}"))
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

/// The inverse of [`os_join`]: map an OS-mount folder path back into a volume's
/// index-path space by stripping the mount root, so the privacy retro-delete can match
/// it against the stored (index-relative) rows. `Some("/Photos")` for
/// `/Volumes/naspi/Photos` under mount `/Volumes/naspi`; `Some("")` for the mount root
/// itself (the whole volume); the OS folder passes through unchanged on a `root`/local
/// volume (mount root `/`). `None` when the folder isn't under this volume's mount at
/// all (a different volume) — the caller skips it.
pub fn os_folder_to_index_prefix(folder: &str, mount_root: &str) -> Option<String> {
    if mount_root == "/" || mount_root.is_empty() {
        return Some(folder.to_string());
    }
    let trimmed = mount_root.trim_end_matches('/');
    if folder == trimmed {
        return Some(String::new());
    }
    folder
        .strip_prefix(trimmed)
        .filter(|rest| rest.starts_with('/'))
        .map(|rest| rest.to_string())
}

/// A scripted fetcher for tests: maps an OS path to bytes, or to a disconnect, so the
/// enrich core's pause/resume paths run with no real mount.
#[cfg(test)]
pub struct FakeByteFetcher {
    bytes: std::collections::HashMap<String, Vec<u8>>,
    disconnected: std::collections::HashSet<String>,
    unreadable: std::collections::HashSet<String>,
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
            unreadable: std::collections::HashSet::new(),
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

    /// Script a per-file read failure (permission denied and friends) for an OS path.
    pub fn unreadable_on(mut self, os_path: impl Into<String>) -> Self {
        self.unreadable.insert(os_path.into());
        self
    }
}

#[cfg(test)]
impl ByteFetcher for FakeByteFetcher {
    fn fetch(&self, os_path: &str, _timeout: Duration) -> Result<Vec<u8>, FetchError> {
        if self.disconnected.contains(os_path) {
            return Err(FetchError::Disconnected("scripted unmount".to_string()));
        }
        if self.unreadable.contains(os_path) {
            return Err(FetchError::Unreadable("scripted per-file read failure".to_string()));
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
    fn os_folder_to_index_prefix_is_the_inverse_of_os_join() {
        // Network volume: strip the mount root to reach the stored index-path space.
        assert_eq!(
            os_folder_to_index_prefix("/Volumes/naspi/Photos", "/Volumes/naspi"),
            Some("/Photos".to_string())
        );
        // The mount root itself ⇒ the whole volume (empty prefix matches every path).
        assert_eq!(
            os_folder_to_index_prefix("/Volumes/naspi", "/Volumes/naspi"),
            Some(String::new())
        );
        // Local / root volume: index path == OS path, so the folder passes through.
        assert_eq!(
            os_folder_to_index_prefix("/Users/me/Documents/IDs", "/"),
            Some("/Users/me/Documents/IDs".to_string())
        );
        // A folder on a DIFFERENT volume isn't under this mount ⇒ None (skip it).
        assert_eq!(os_folder_to_index_prefix("/Volumes/other/x", "/Volumes/naspi"), None);
        // A name-prefix sibling is NOT within the mount.
        assert_eq!(os_folder_to_index_prefix("/Volumes/naspi2/x", "/Volumes/naspi"), None);
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

    /// THE M1 classification fix: a permission-denied file is a PER-FILE fault
    /// (skip-and-count), never a "disconnected" (which pauses the whole pass —
    /// the TCC-EPERM-stalls-the-NAS bug this distinction exists for).
    #[test]
    #[cfg(unix)]
    fn fs_fetch_permission_denied_is_unreadable_not_disconnected() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join("locked.jpg");
        std::fs::write(&path, b"jpeg bytes").expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).expect("chmod");

        let err = FsByteFetcher
            .fetch(&path.to_string_lossy(), Duration::from_secs(5))
            .expect_err("an unreadable file errors");
        // Restore permissions so the tempdir cleanup can delete it.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod back");
        assert!(
            matches!(err, FetchError::Unreadable(_)),
            "EACCES must classify as a per-file Unreadable, got {err:?}"
        );
    }

    /// The errno line between "this FILE is bad" and "the MOUNT is gone": network-
    /// transport errnos pause (disconnect), anything else skips (unreadable).
    #[test]
    fn io_errors_classify_by_errno_kind() {
        use std::io::{Error, ErrorKind};
        // Typed transport-loss errnos ⇒ Disconnected (pause, resume on reconnect).
        for errno in [
            libc::ETIMEDOUT,
            libc::ENOTCONN,
            libc::ENETDOWN,
            libc::ENETUNREACH,
            libc::EHOSTDOWN,
            libc::EHOSTUNREACH,
            libc::ENODEV,
            libc::ENXIO,
            libc::ESTALE,
        ] {
            let classified = classify_io_error(&Error::from_raw_os_error(errno), "/m/x.jpg", "read");
            assert!(
                matches!(classified, FetchError::Disconnected(_)),
                "errno {errno} is transport loss, got {classified:?}"
            );
        }
        // Per-file errnos ⇒ Unreadable (skip-and-count, never a pause).
        for errno in [libc::EACCES, libc::EPERM, libc::EISDIR, libc::EIO] {
            let classified = classify_io_error(&Error::from_raw_os_error(errno), "/m/x.jpg", "read");
            assert!(
                matches!(classified, FetchError::Unreadable(_)),
                "errno {errno} is a per-file fault, got {classified:?}"
            );
        }
        // NotFound stays its own variant (vanished source; GC collects it).
        let nf = classify_io_error(&Error::from(ErrorKind::NotFound), "/m/x.jpg", "open");
        assert!(matches!(nf, FetchError::NotFound));
    }
}

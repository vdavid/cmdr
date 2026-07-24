//! The byte-fetch seam for network enrichment: read one image's compressed bytes off
//! an opted-in network volume, bounded against an indefinitely-blocking transport.
//!
//! ## Byte-fetch decision (plan M1): the app's own session first, OS mount as fallback
//!
//! Media enrichment MUST read image bytes off the wire — unlike `importance/`, which
//! never does. Two fetchers behind one [`ByteFetcher`] seam, picked per pass by
//! `Volume::supports_local_fs_access()` (the same predicate the archive backend uses
//! for its local-vs-remote byte source):
//!
//! - [`VolumeByteFetcher`] for a volume the app holds its OWN transport session to
//!   (a Direct-smb2 `SmbVolume`): reads via `Volume::open_read_stream_for_scan` (over
//!   SMB, small hinted files come from the scan-session connection pool, so the
//!   parallel pass's fan-out reads genuinely overlap).
//!   The OS mount is deliberately avoided there: macOS TCC ("network volumes")
//!   denies `std::fs` on `/Volumes/…` for unsigned dev binaries and can regress
//!   per-binary on rebuilds, while the direct session is the connection Cmdr
//!   already owns, health-checks, and auto-reconnects — and it yields TYPED errors
//!   (`VolumeError::DeviceDisconnected`), so pause-vs-skip classification doesn't
//!   ride errno guesswork.
//! - [`FsByteFetcher`] for mount-only volumes (no Direct session): `std::fs` on the
//!   OS mount path, failures classified by typed errno ([`classify_io_error`]).
//!
//! **Non-blocking discipline.** A network read can block indefinitely on a dead/hung
//! transport. [`FsByteFetcher`] runs the read on a throwaway thread and waits with a
//! timeout; [`VolumeByteFetcher`] wraps the async read in `tokio::time::timeout`. A
//! timeout returns [`FetchError::Disconnected`] rather than wedging the pass.
//! Critically, the fetch happens in the enrich layer — NOT on the serialized Vision
//! OCR worker thread — so a hung transport can never stall OCR of other (local)
//! volumes.

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
    /// Read the bytes at `os_path`, giving up after `timeout`. `size_hint` is the
    /// index's last-known file size: the direct fetcher forwards it to
    /// `Volume::open_read_stream_with_hint` (SMB's one-round-trip compound read for
    /// small files) and short-circuits an over-cap file without reading; a stale
    /// hint is harmless (the read self-corrects).
    fn fetch(&self, os_path: &str, size_hint: Option<u64>, timeout: Duration) -> Result<Vec<u8>, FetchError>;
}

/// The production fetcher: `std::fs::read` on the OS mount path, on a throwaway thread
/// bounded by a timeout so a hung mount can't block the pass.
pub struct FsByteFetcher;

impl ByteFetcher for FsByteFetcher {
    fn fetch(&self, os_path: &str, _size_hint: Option<u64>, timeout: Duration) -> Result<Vec<u8>, FetchError> {
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

/// The direct-session fetcher: read through the `Volume` trait (the app's OWN smb2
/// session — no TCC, no foreign-mount surprises), bridging the async read onto the
/// caller's (blocking) thread. Constructed per pass for volumes with
/// `supports_local_fs_access() == false`; the enrichment fetch runs on a
/// `spawn_blocking` / plain worker thread, never a runtime worker, so `block_on`
/// can't reenter the executor (the same bridge as the archive backend's
/// `VolumeByteSource`).
pub struct VolumeByteFetcher {
    volume: std::sync::Arc<dyn crate::file_system::volume::Volume>,
    /// The tokio runtime the async volume read runs under; captured at
    /// construction (inside the runtime context) because the fetch itself runs on
    /// plain threads with no ambient runtime.
    handle: tokio::runtime::Handle,
}

impl VolumeByteFetcher {
    /// A fetcher reading through `volume` on the runtime behind `handle`.
    pub fn new(volume: std::sync::Arc<dyn crate::file_system::volume::Volume>, handle: tokio::runtime::Handle) -> Self {
        Self { volume, handle }
    }
}

impl ByteFetcher for VolumeByteFetcher {
    fn fetch(&self, os_path: &str, size_hint: Option<u64>, timeout: Duration) -> Result<Vec<u8>, FetchError> {
        // A known-oversized file is skipped without touching the wire at all.
        if size_hint.is_some_and(|s| s > MAX_FETCH_BYTES) {
            return Err(FetchError::TooLarge);
        }
        let volume = std::sync::Arc::clone(&self.volume);
        let path = std::path::PathBuf::from(os_path);
        // `Volume` impls accept mount-absolute display paths (SmbVolume strips its
        // mount prefix; LocalPosix resolves absolutes), so the enrich layer's
        // os-joined path passes through unchanged.
        self.handle.block_on(async move {
            match tokio::time::timeout(timeout, read_via_volume(volume.as_ref(), &path, size_hint)).await {
                Ok(result) => result,
                // The outer timeout is the hung-transport backstop: classify as a
                // disconnect (pause), never a per-file fault. Dropping the timed-out
                // future cancels the in-flight SMB read (safe on smb2; enrichment
                // never runs on MTP, where a dropped round trip wedges the device).
                Err(_) => Err(FetchError::Disconnected(format!(
                    "volume read timed out after {timeout:?}"
                ))),
            }
        })
    }
}

/// Drain one file through `Volume::open_read_stream_for_scan` (the background bulk
/// read: over SMB, small hinted files come from the scan-session connection pool so
/// parallel prefetch reads actually overlap), capping at [`MAX_FETCH_BYTES`] (stop
/// draining and skip, rather than buffering a pathological file), classifying
/// failures by TYPED `VolumeError` variant.
async fn read_via_volume(
    volume: &dyn crate::file_system::volume::Volume,
    path: &std::path::Path,
    size_hint: Option<u64>,
) -> Result<Vec<u8>, FetchError> {
    let mut stream = volume
        .open_read_stream_for_scan(path, size_hint)
        .await
        .map_err(classify_volume_error)?;
    // Pre-size from the hint (capped), so a typical photo lands in one allocation.
    let mut buf = Vec::with_capacity(size_hint.unwrap_or(0).min(MAX_FETCH_BYTES) as usize);
    while let Some(chunk) = stream.next_chunk().await {
        let chunk = chunk.map_err(classify_volume_error)?;
        buf.extend_from_slice(&chunk);
        if buf.len() as u64 > MAX_FETCH_BYTES {
            // Dropping the stream cancels the producer (SmbReadStream sends its
            // cancel signal on drop).
            return Err(FetchError::TooLarge);
        }
    }
    Ok(buf)
}

/// Classify a `VolumeError` from the direct read path — TYPED variants only, the
/// whole point of reading through the session Cmdr owns:
///
/// - `NotFound` ⇒ a vanished source (skip; GC collects it after a completed scan).
/// - `DeviceDisconnected` / `ConnectionTimeout` ⇒ the transport is gone (pause the
///   pass; the registration bus resumes it on reconnect).
/// - Everything else (`PermissionDenied`, `IsADirectory`, `IoError`, and MTP's
///   `DeviceSessionReset`, which its docs forbid mapping to a disconnect) ⇒ a
///   per-file [`FetchError::Unreadable`]: skip-and-count, never a pause.
fn classify_volume_error(e: crate::file_system::volume::VolumeError) -> FetchError {
    use crate::file_system::volume::VolumeError;
    match e {
        VolumeError::NotFound(_) => FetchError::NotFound,
        VolumeError::DeviceDisconnected(msg) => FetchError::Disconnected(msg),
        VolumeError::ConnectionTimeout(msg) => FetchError::Disconnected(format!("connection timeout: {msg}")),
        other => FetchError::Unreadable(other.to_string()),
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
    fn fetch(&self, os_path: &str, _size_hint: Option<u64>, _timeout: Duration) -> Result<Vec<u8>, FetchError> {
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
            .fetch(&path.to_string_lossy(), None, Duration::from_secs(5))
            .expect("fetch");
        assert_eq!(bytes, b"hello bytes");
    }

    #[test]
    fn fs_fetch_missing_file_is_not_found() {
        let err = FsByteFetcher
            .fetch("/nonexistent/cmdr/media/x.jpg", None, Duration::from_secs(5))
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
            .fetch(&path.to_string_lossy(), None, Duration::from_secs(5))
            .expect_err("an unreadable file errors");
        // Restore permissions so the tempdir cleanup can delete it.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod back");
        assert!(
            matches!(err, FetchError::Unreadable(_)),
            "EACCES must classify as a per-file Unreadable, got {err:?}"
        );
    }

    /// The direct-session fetcher reads whole files through the `Volume` trait
    /// (the M1 read-path fix), from a plain thread with no ambient runtime — the
    /// exact shape of the enrichment pass's `spawn_blocking` / fetcher threads.
    #[test]
    fn volume_fetch_reads_bytes_through_the_volume_trait() {
        use crate::file_system::volume::Volume;
        use crate::file_system::volume::backends::InMemoryVolume;
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let volume = std::sync::Arc::new(InMemoryVolume::new("test"));
        rt.block_on(volume.create_file(std::path::Path::new("/DCIM/a.jpg"), b"direct bytes"))
            .expect("seed");
        let fetcher = VolumeByteFetcher::new(volume, rt.handle().clone());

        let bytes = std::thread::scope(|s| {
            s.spawn(|| fetcher.fetch("/DCIM/a.jpg", Some(12), Duration::from_secs(5)))
                .join()
                .expect("thread")
        })
        .expect("fetch");
        assert_eq!(bytes, b"direct bytes");
    }

    /// Typed classification end to end on the direct path: a vanished file is
    /// `NotFound` (skip; GC collects it), and a known-oversized file is `TooLarge`
    /// WITHOUT touching the wire.
    #[test]
    fn volume_fetch_classifies_not_found_and_oversize_hint() {
        use crate::file_system::volume::backends::InMemoryVolume;
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let volume = std::sync::Arc::new(InMemoryVolume::new("test"));
        let fetcher = VolumeByteFetcher::new(volume, rt.handle().clone());

        let err = fetcher
            .fetch("/gone.jpg", Some(10), Duration::from_secs(5))
            .expect_err("missing file");
        assert!(matches!(err, FetchError::NotFound), "got {err:?}");

        let err = fetcher
            .fetch("/whatever.jpg", Some(MAX_FETCH_BYTES + 1), Duration::from_secs(5))
            .expect_err("over-cap hint");
        assert!(matches!(err, FetchError::TooLarge), "got {err:?}");
    }

    /// The `VolumeError` line between pause and skip: only a typed
    /// `DeviceDisconnected` / `ConnectionTimeout` pauses; every other volume error
    /// (permission, I/O, MTP's session reset) is a per-file skip.
    #[test]
    fn volume_errors_classify_disconnect_vs_unreadable_by_typed_variant() {
        use crate::file_system::volume::VolumeError;
        assert!(matches!(
            classify_volume_error(VolumeError::DeviceDisconnected("gone".into())),
            FetchError::Disconnected(_)
        ));
        assert!(matches!(
            classify_volume_error(VolumeError::ConnectionTimeout("slow".into())),
            FetchError::Disconnected(_)
        ));
        assert!(matches!(
            classify_volume_error(VolumeError::NotFound("x".into())),
            FetchError::NotFound
        ));
        for per_file in [
            VolumeError::PermissionDenied("locked".into()),
            VolumeError::IsADirectory("dir".into()),
            VolumeError::IoError {
                message: "bad sector".into(),
                raw_os_error: Some(5),
            },
            // MTP-only today, and its docs forbid mapping it to a disconnect.
            VolumeError::DeviceSessionReset("reset".into()),
        ] {
            let classified = classify_volume_error(per_file);
            assert!(
                matches!(classified, FetchError::Unreadable(_)),
                "expected Unreadable, got {classified:?}"
            );
        }
    }

    /// A mid-stream typed disconnect (the cable-yank case) surfaces as
    /// `Disconnected` even after good chunks arrived — a partial read must never
    /// pass itself off as the file's bytes or as a per-file fault.
    #[test]
    fn volume_fetch_maps_a_mid_stream_disconnect_to_disconnected() {
        use crate::file_system::volume::{Volume, VolumeError, VolumeReadStream};
        use std::pin::Pin;

        struct YankedStream {
            sent: bool,
        }
        impl VolumeReadStream for YankedStream {
            fn next_chunk(
                &mut self,
            ) -> Pin<Box<dyn std::future::Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>>
            {
                Box::pin(async move {
                    if self.sent {
                        Some(Err(VolumeError::DeviceDisconnected("cable yanked".into())))
                    } else {
                        self.sent = true;
                        Some(Ok(vec![0u8; 1024]))
                    }
                })
            }
            fn total_size(&self) -> u64 {
                4096
            }
            fn bytes_read(&self) -> u64 {
                if self.sent { 1024 } else { 0 }
            }
        }

        struct YankedVolume;
        impl Volume for YankedVolume {
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
            fn name(&self) -> &str {
                "yanked"
            }
            fn root(&self) -> &std::path::Path {
                std::path::Path::new("/")
            }
            fn list_directory<'a>(
                &'a self,
                _path: &'a std::path::Path,
                _on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
            ) -> Pin<
                Box<
                    dyn std::future::Future<Output = Result<Vec<crate::file_system::FileEntry>, VolumeError>>
                        + Send
                        + 'a,
                >,
            > {
                Box::pin(async { Ok(Vec::new()) })
            }
            fn get_metadata<'a>(
                &'a self,
                _path: &'a std::path::Path,
            ) -> Pin<
                Box<dyn std::future::Future<Output = Result<crate::file_system::FileEntry, VolumeError>> + Send + 'a>,
            > {
                Box::pin(async { Err(VolumeError::NotSupported) })
            }
            fn exists<'a>(
                &'a self,
                _path: &'a std::path::Path,
            ) -> Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
                Box::pin(async { true })
            }
            fn is_directory<'a>(
                &'a self,
                _path: &'a std::path::Path,
            ) -> Pin<Box<dyn std::future::Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
                Box::pin(async { Ok(false) })
            }
            fn open_read_stream<'a>(
                &'a self,
                _path: &'a std::path::Path,
            ) -> Pin<Box<dyn std::future::Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>>
            {
                Box::pin(async { Ok(Box::new(YankedStream { sent: false }) as Box<dyn VolumeReadStream>) })
            }
        }

        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let fetcher = VolumeByteFetcher::new(std::sync::Arc::new(YankedVolume), rt.handle().clone());
        let err = fetcher
            .fetch("/DCIM/a.jpg", Some(4096), Duration::from_secs(5))
            .expect_err("yank errors");
        assert!(matches!(err, FetchError::Disconnected(_)), "got {err:?}");
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

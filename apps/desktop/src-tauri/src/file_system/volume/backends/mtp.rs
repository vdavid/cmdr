//! MTP (Media Transfer Protocol) volume implementation.
//!
//! Wraps MTP device storage as a Volume, enabling MTP browsing through
//! the standard file listing pipeline (same icons, sorting, view modes as local files).

use super::{VolumeError, VolumeReadStream};
use crate::mtp::connection::{MtpConnectionError, MtpReadSession, connection_manager};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use tokio_util::sync::CancellationToken;

/// Bridges Cmdr's `CancellationToken` to mtp-rs's poll-based `CancelToken` for
/// the duration of one call.
///
/// mtp-rs checks an `Arc<AtomicBool>` between PTP roundtrips, so something has
/// to mirror the token into it. A task parked on `cancelled()` costs nothing
/// while it waits (no polling), and the guard cancels its own child token when
/// the bridge drops, which retires that task at the end of every call — clean
/// exit, cancel, and error alike.
///
/// Live only for calls the caller actually made cancelable: [`Self::open`]
/// returns `None` for `None`, and the backend then passes no token to mtp-rs,
/// exactly as before.
struct MtpCancelBridge {
    token: mtp_rs::CancelToken,
    _retire: tokio_util::sync::DropGuard,
}

impl MtpCancelBridge {
    fn open(cancel: Option<&CancellationToken>) -> Option<Self> {
        let cancel = cancel?;
        let token = mtp_rs::CancelToken::new();
        // A CHILD token, so dropping the bridge retires the mirror task without
        // touching the caller's token (which outlives this one call).
        let scoped = cancel.child_token();
        let watch = scoped.clone();
        let mirror = token.clone();
        tokio::spawn(async move {
            watch.cancelled().await;
            mirror.cancel();
        });
        Some(Self {
            token,
            _retire: scoped.drop_guard(),
        })
    }

    fn token(&self) -> &mtp_rs::CancelToken {
        &self.token
    }
}

/// A volume backed by an MTP device storage.
///
/// This implementation wraps the MTP connection manager to provide file system
/// abstraction. All methods are natively async: MTP operations go through the
/// connection manager which uses async USB bulk transfers.
pub struct MtpVolume {
    /// Display name (typically the storage description like "Internal storage")
    name: String,
    /// MTP device ID (for example, "mtp-20-5")
    pub(super) device_id: String,
    /// Storage ID within the device
    pub(super) storage_id: u32,
    /// Virtual root path for this volume (for example, "/mtp-20-5/65537")
    root: PathBuf,
    /// Volume ID for listing cache lookups (format: "{device_id}:{storage_id}").
    volume_id: String,
}

impl MtpVolume {
    /// Creates a new MTP volume for a specific device storage.
    ///
    /// # Arguments
    /// * `device_id` - The MTP device ID (format: "mtp-{bus}-{address}")
    /// * `storage_id` - The storage ID within the device
    /// * `name` - Display name for the storage (for example, "Internal shared storage")
    pub fn new(device_id: &str, storage_id: u32, name: &str) -> Self {
        let volume_id = format!("{}:{}", device_id, storage_id);
        Self {
            name: name.to_string(),
            device_id: device_id.to_string(),
            storage_id,
            root: PathBuf::from(format!("mtp://{}/{}", device_id, storage_id)),
            volume_id,
        }
    }

    /// Converts a Volume path to an MTP inner path.
    ///
    /// The path can be in several formats:
    /// - MTP URL: `mtp://mtp-0-1/65537` or `mtp://mtp-0-1/65537/DCIM/Camera`
    /// - Absolute path: `/DCIM/Camera`
    /// - Relative path: `DCIM/Camera`
    ///
    /// The MTP API expects paths relative to the storage root (for example, `DCIM/Camera`).
    pub(super) fn to_mtp_path(&self, path: &Path) -> String {
        let path_str = path.to_string_lossy();

        // Handle MTP URLs (mtp://device-id/storage-id/optional/path)
        if path_str.starts_with("mtp://") {
            // Parse: mtp://mtp-0-1/65537/DCIM/Camera -> DCIM/Camera
            // The format is: mtp://{device_id}/{storage_id}/{path}
            let without_scheme = path_str.strip_prefix("mtp://").unwrap_or(&path_str);

            // Find the device_id/storage_id prefix and skip it
            // Device ID format: mtp-{bus}-{address} (like mtp-0-1)
            // So we need to skip: device_id/storage_id/
            let parts: Vec<&str> = without_scheme.splitn(3, '/').collect();
            // parts[0] = device_id (like "mtp-0-1")
            // parts[1] = storage_id (like "65537")
            // parts[2] = inner path (like "DCIM/Camera") or absent for root

            return if parts.len() >= 3 {
                parts[2].to_string()
            } else {
                String::new() // Root of storage
            };
        }

        // Handle empty or root paths
        if path_str.is_empty() || path_str == "/" || path_str == "." {
            return String::new();
        }

        // Strip leading slash if present
        path_str.strip_prefix('/').unwrap_or(&path_str).to_string()
    }

    /// Normalizes any caller-supplied path on this volume to the canonical
    /// absolute MTP URL (`mtp://{device_id}/{storage_id}[/inner/path]`).
    ///
    /// `notify_mutation` passes this as the PARENT path to
    /// `notify_directory_changed`, which finds the target `LISTING_CACHE` entry by
    /// exact path equality against `CachedListing.path` — and that IS the absolute
    /// URL (pane navigation feeds the URL into the listing pipeline). Write/delete
    /// callers, however, may hand us a volume-relative path (e.g. `/file-a.txt`
    /// after the cross-volume copy orchestrator does `dest_path.join(name)` with
    /// `dest_path = "/"`); without this conversion the listing lookup misses and
    /// the cache patch is silently dropped, leaving the pane stale.
    ///
    /// Note the per-ENTRY paths INSIDE a listing are the storage-relative inner
    /// form (`/Documents/notes.txt`), NOT the URL — so the `Removed` patch matches
    /// entries by NAME, not full path (see `caching::remove_entry_by_name`).
    fn to_url_path(&self, path: &Path) -> PathBuf {
        let path_str = path.to_string_lossy();
        if path_str.starts_with("mtp://") {
            return path.to_path_buf();
        }
        let inner = self.to_mtp_path(path);
        if inner.is_empty() {
            self.root.clone()
        } else {
            self.root.join(inner)
        }
    }
}

/// Adapts a `VolumeReadStream` into a `futures::Stream` that mtp-rs can
/// consume lazily, calling `on_progress` after each chunk and surfacing
/// `ControlFlow::Break` as an `io::Error` so the upload unwinds promptly.
///
/// Pre-fix this loop was missing entirely: `write_from_stream` collected
/// every chunk into a `Vec<Bytes>` before any USB write began (OOM risk for
/// large files) and never invoked the transfer progress / cancel callback.
pub(super) fn volume_read_stream_to_chunk_stream<'a>(
    stream: Box<dyn VolumeReadStream>,
    total: u64,
    on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
) -> impl futures_util::Stream<Item = Result<bytes::Bytes, std::io::Error>> + Send + 'a {
    futures_util::stream::unfold(
        (stream, 0u64, on_progress, total),
        |(mut stream, bytes_written, on_progress, total)| async move {
            match stream.next_chunk().await {
                Some(Ok(chunk)) => {
                    let new_total = bytes_written + chunk.len() as u64;
                    if on_progress(new_total, total) == std::ops::ControlFlow::Break(()) {
                        let err = std::io::Error::new(std::io::ErrorKind::Interrupted, "Operation cancelled");
                        return Some((Err(err), (stream, new_total, on_progress, total)));
                    }
                    Some((Ok(bytes::Bytes::from(chunk)), (stream, new_total, on_progress, total)))
                }
                Some(Err(e)) => {
                    let err = std::io::Error::other(e.to_string());
                    Some((Err(err), (stream, bytes_written, on_progress, total)))
                }
                None => None,
            }
        },
    )
}

/// Bytes-per-window for a [`MtpReadStream`]. Production uses
/// [`crate::mtp::connection::MTP_READ_WINDOW`]; tests shrink it via
/// [`test_window`] so a small fixture spans multiple windows.
fn mtp_read_window() -> u32 {
    #[cfg(test)]
    {
        let o = test_window::get();
        if o != 0 {
            return o;
        }
    }
    crate::mtp::connection::MTP_READ_WINDOW
}

/// Test-only override for the read window size (see [`mtp_read_window`]). A
/// global is harmless here: every read test wants a small window, and the
/// production default is never asserted, so a value set by one test never
/// breaks another. Unit tests construct [`MtpReadStream`] with an explicit
/// `window` instead and don't touch this.
// `pub(super)` so the sibling `mtp_test` module (a child of `backends`) can
// reach it; `set` is widened to the same scope, while `get` stays `pub(super)`
// to `test_window` because only the in-file `mtp_read_window` reads it.
#[cfg(test)]
pub(super) mod test_window {
    use std::sync::atomic::{AtomicU32, Ordering};

    static OVERRIDE: AtomicU32 = AtomicU32::new(0);

    pub(super) fn get() -> u32 {
        OVERRIDE.load(Ordering::Relaxed)
    }

    #[cfg(feature = "virtual-mtp")]
    pub(in crate::file_system::volume::backends) fn set(window: u32) {
        OVERRIDE.store(window, Ordering::Relaxed);
    }
}

/// Bounded-window MTP read stream.
///
/// Reads a file as a sequence of bounded `GetPartialObject64` windows instead of
/// one held-open `GetObject`. Between windows nothing is in flight and the
/// one-per-device PTP session is free, so a foreground listing slips in at window
/// granularity (the whole point — navigate the phone during a copy).
///
/// `next_chunk` delegates to the connection layer's `read_next_window`, which
/// takes the per-device lock for each `GetPartialObject64`. The window
/// bookkeeping (total size, offset, clamp-to-remaining, EOF, advance-by-returned-
/// length, the 0-byte-before-EOF stall guard) lives in mtp-rs's `WindowedDownload`
/// inside the cached [`MtpReadSession`]; this struct just relays windows and
/// reports progress.
struct MtpReadStream {
    session: MtpReadSession,
    device_id: String,
}

impl VolumeReadStream for MtpReadStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            match connection_manager()
                .read_next_window(&mut self.session, &self.device_id)
                .await
            {
                Ok(Some(bytes)) => Some(Ok(bytes)),
                Ok(None) => None,
                Err(e) => Some(Err(map_mtp_error(e))),
            }
        })
    }

    fn total_size(&self) -> u64 {
        self.session.total_size()
    }

    fn bytes_read(&self) -> u64 {
        self.session.bytes_read()
    }

    // `cancel_and_release` uses the trait default (no-op): bounded windows hold
    // nothing between reads, so there's no in-flight transaction to abort. A
    // window read in flight when the stream is dropped self-heals via mtp-rs's
    // `TransactionScope` (see the connection layer's `read_next_window`).
}

/// Maps MTP connection errors to Volume errors.
/// `ENOTEMPTY`, which POSIX numbers differently per platform. MTP builds on
/// macOS and Linux only, so those are the two that exist.
#[cfg(target_os = "linux")]
const ENOTEMPTY: i32 = 39;
#[cfg(not(target_os = "linux"))]
const ENOTEMPTY: i32 = 66;

fn map_mtp_error(e: MtpConnectionError) -> VolumeError {
    match e {
        MtpConnectionError::DeviceNotFound { .. } | MtpConnectionError::NotConnected { .. } => {
            VolumeError::NotFound(e.to_string())
        }
        MtpConnectionError::ObjectNotFound { path, .. } => VolumeError::NotFound(path),
        MtpConnectionError::StaleParentHandle { dest_folder, .. } => VolumeError::StaleDestinationHandle(dest_folder),
        MtpConnectionError::ExclusiveAccess { .. } | MtpConnectionError::PermissionDenied { .. } => {
            VolumeError::PermissionDenied(e.to_string())
        }
        MtpConnectionError::Cancelled { .. } => VolumeError::Cancelled(e.to_string()),
        MtpConnectionError::Disconnected { .. } => VolumeError::DeviceDisconnected(e.to_string()),
        // ❌ NOT `DeviceDisconnected`: a session reset leaves the device plugged
        // in and reopenable, so tearing down the volume would throw away a live
        // device. It's a RECOVERABLE failure of this one operation — the
        // connection layer already has a reopen running — so it carries its own
        // retryable variant rather than a dead-end `IoError`.
        MtpConnectionError::SessionReset { .. } => VolumeError::DeviceSessionReset(e.to_string()),
        MtpConnectionError::Timeout { .. } => VolumeError::ConnectionTimeout(e.to_string()),
        MtpConnectionError::StorageFull { .. } => VolumeError::StorageFull { message: e.to_string() },
        MtpConnectionError::StoreReadOnly { .. } => VolumeError::ReadOnly(e.to_string()),
        // The trait contract's refusal, carrying the errno POSIX would have
        // raised, so a caller that classifies on `raw_os_error` sees the same
        // thing here as it does over `LocalPosixVolume` or SMB.
        MtpConnectionError::DirectoryNotEmpty { .. } => VolumeError::IoError {
            message: e.to_string(),
            raw_os_error: Some(ENOTEMPTY),
        },
        _ => VolumeError::IoError {
            message: e.to_string(),
            raw_os_error: None,
        },
    }
}

/// Test-only call counter for `MtpVolume::list_directory`. The
/// `scan_for_copy_batch_with_progress` integration tests assert "exactly 2
/// `list_directory` calls for 2 unique parents" without having to wrap the
/// volume (the override calls `self.list_directory` via static dispatch on
/// `MtpVolume`, so a wrapper Volume can't intercept it).
#[cfg(test)]
// Visible at `crate::file_system::volume::mtp_scan_oracle_tests`: those oracle
// tests live one level up (in `volume`), so they need this wider scope rather
// than a `pub(super)` that would only reach `backends`.
pub(in crate::file_system::volume) mod test_hooks {
    // The two readers below are called from the oracle tests one level up, which a
    // partial test build may not compile; `deny(unused)` flags them either way.
    #![allow(dead_code, reason = "read by the `mtp_scan_oracle_tests` module one level up")]

    use std::sync::atomic::{AtomicUsize, Ordering};

    static LIST_DIRECTORY_CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

    pub(super) fn bump_list_directory_call_count() {
        LIST_DIRECTORY_CALL_COUNT.fetch_add(1, Ordering::Relaxed);
    }

    pub fn reset_list_directory_call_count() {
        LIST_DIRECTORY_CALL_COUNT.store(0, Ordering::Relaxed);
    }

    pub fn list_directory_call_count() -> usize {
        LIST_DIRECTORY_CALL_COUNT.load(Ordering::Relaxed)
    }
}

/// The `Volume` impl, in its own file so neither half runs long.
mod volume_impl;

/// The scan surface (copy scan, batch scan, conflict scan).
mod scan;

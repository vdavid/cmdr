//! Tests for `MtpVolume`.
//!
//! Sibling test module declared in `backends/mod.rs` (a child of `backends`,
//! mirroring `local_posix_test.rs` / `in_memory_test.rs`), so `super::*` reaches
//! the backend re-exports and `super::mtp::…` reaches the MTP backend's
//! `pub(super)` internals (`to_mtp_path`, the `device_id` / `storage_id` fields,
//! `volume_read_stream_to_chunk_stream`, and the `test_window` override).

use super::mtp::volume_read_stream_to_chunk_stream;
use super::*;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

#[cfg(feature = "virtual-mtp")]
use super::mtp::test_window;
#[cfg(feature = "virtual-mtp")]
use crate::mtp::connection::{MtpConnectionError, connection_manager};

#[test]
fn test_new_creates_volume() {
    let vol = MtpVolume::new("mtp-20-5", 65537, "Internal storage");
    assert_eq!(vol.name(), "Internal storage");
    assert_eq!(vol.device_id, "mtp-20-5");
    assert_eq!(vol.storage_id, 65537);
}

#[test]
fn test_root_path() {
    let vol = MtpVolume::new("mtp-20-5", 65537, "Internal storage");
    assert_eq!(vol.root().to_string_lossy(), "mtp://mtp-20-5/65537");
}

#[test]
fn test_to_mtp_path_empty() {
    let vol = MtpVolume::new("mtp-20-5", 65537, "Test");
    assert_eq!(vol.to_mtp_path(Path::new("")), "");
    assert_eq!(vol.to_mtp_path(Path::new("/")), "");
    assert_eq!(vol.to_mtp_path(Path::new(".")), "");
}

#[test]
fn test_to_mtp_path_relative() {
    let vol = MtpVolume::new("mtp-20-5", 65537, "Test");
    assert_eq!(vol.to_mtp_path(Path::new("DCIM")), "DCIM");
    assert_eq!(vol.to_mtp_path(Path::new("DCIM/Camera")), "DCIM/Camera");
}

#[test]
fn test_to_mtp_path_absolute() {
    let vol = MtpVolume::new("mtp-20-5", 65537, "Test");
    assert_eq!(vol.to_mtp_path(Path::new("/DCIM")), "DCIM");
    assert_eq!(vol.to_mtp_path(Path::new("/DCIM/Camera")), "DCIM/Camera");
}

#[test]
fn test_to_mtp_path_mtp_url_root() {
    let vol = MtpVolume::new("mtp-0-1", 65537, "Test");
    // MTP URL for storage root
    assert_eq!(vol.to_mtp_path(Path::new("mtp://mtp-0-1/65537")), "");
}

#[test]
fn test_to_mtp_path_mtp_url_with_path() {
    let vol = MtpVolume::new("mtp-0-1", 65537, "Test");
    // MTP URL with nested path
    assert_eq!(vol.to_mtp_path(Path::new("mtp://mtp-0-1/65537/DCIM")), "DCIM");
    assert_eq!(
        vol.to_mtp_path(Path::new("mtp://mtp-0-1/65537/DCIM/Camera")),
        "DCIM/Camera"
    );
}

#[test]
fn test_supports_watching_returns_false() {
    // MTP volumes return false for supports_watching because they have their
    // own event loop (in MtpConnectionManager) that handles file watching
    // independently. The supports_watching check in operations.rs is only
    // for the local notify-based watcher, which doesn't work for MTP paths.
    let vol = MtpVolume::new("mtp-20-5", 65537, "Test");
    assert!(!vol.supports_watching());
}

#[test]
fn test_supports_streaming_returns_true() {
    // MTP volumes support streaming for direct MTP-to-MTP transfers.
    let vol = MtpVolume::new("mtp-20-5", 65537, "Test");
    assert!(vol.supports_streaming());
}

/// Regression for the high-severity audit finding: pre-fix, MtpVolume's
/// `write_from_stream` was named `_on_progress` (signaling unused) and
/// drained the entire source into a `Vec<Bytes>` before any USB write.
/// Both behaviors are tested via the extracted stream adapter helper,
/// which is what `write_from_stream` now drives.
#[tokio::test]
async fn volume_read_stream_to_chunk_stream_calls_on_progress_per_chunk() {
    use futures_util::StreamExt;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct MockStream {
        chunks: std::vec::IntoIter<Vec<u8>>,
        total: u64,
        read: u64,
    }
    impl VolumeReadStream for MockStream {
        fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
            Box::pin(async move {
                self.chunks.next().map(|c| {
                    self.read += c.len() as u64;
                    Ok(c)
                })
            })
        }
        fn total_size(&self) -> u64 {
            self.total
        }
        fn bytes_read(&self) -> u64 {
            self.read
        }
    }

    let chunks = vec![vec![0u8; 64], vec![0u8; 64], vec![0u8; 64], vec![0u8; 64]];
    let total: u64 = chunks.iter().map(|c| c.len() as u64).sum();
    let stream = Box::new(MockStream {
        chunks: chunks.into_iter(),
        total,
        read: 0,
    });

    let calls = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&calls);
    let on_progress = move |_bytes, _total| {
        counter.fetch_add(1, Ordering::SeqCst);
        std::ops::ControlFlow::Continue(())
    };

    let mut adapter = Box::pin(volume_read_stream_to_chunk_stream(stream, total, &on_progress));
    let mut emitted = 0u64;
    while let Some(chunk) = adapter.next().await {
        emitted += chunk.expect("chunk should be Ok").len() as u64;
    }

    assert_eq!(emitted, total, "all bytes should be forwarded to the upload");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        4,
        "on_progress must fire once per chunk (4 chunks emitted)"
    );
}

/// Companion regression: `ControlFlow::Break(())` must unwind the upload
/// promptly. Pre-fix, the callback was never invoked, so a Cancel could
/// only stop the loop *above* the upload — not the upload itself.
#[tokio::test]
async fn volume_read_stream_to_chunk_stream_surfaces_cancellation() {
    use futures_util::StreamExt;

    struct InfiniteStream {
        total: u64,
        read: u64,
    }
    impl VolumeReadStream for InfiniteStream {
        fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
            Box::pin(async move {
                self.read += 64;
                Some(Ok(vec![0u8; 64]))
            })
        }
        fn total_size(&self) -> u64 {
            self.total
        }
        fn bytes_read(&self) -> u64 {
            self.read
        }
    }

    let stream = Box::new(InfiniteStream {
        total: u64::MAX,
        read: 0,
    });
    let on_progress = |_bytes, _total| std::ops::ControlFlow::Break(());

    let mut adapter = Box::pin(volume_read_stream_to_chunk_stream(stream, u64::MAX, &on_progress));
    let first = adapter.next().await.expect("adapter should yield once");
    assert!(first.is_err(), "Break(()) must produce an io::Error item");
    assert_eq!(
        first.unwrap_err().kind(),
        std::io::ErrorKind::Interrupted,
        "cancellation must surface as Interrupted"
    );
}

#[test]
fn test_listing_is_watched_false_when_device_not_connected() {
    // Without `virtual-mtp`, we can still assert the negative case: a freshly
    // created `MtpVolume` whose device_id was never connected returns false.
    let vol = MtpVolume::new("mtp-never-connected-9999", 65537, "Test");
    assert!(!vol.listing_is_watched(Path::new("/DCIM")));
}

/// Connects to a virtual MTP device, asserts the oracle gate flips true, then
/// disconnects and asserts it flips false. Requires the `virtual-mtp` feature.
#[cfg(feature = "virtual-mtp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_listing_is_watched_flips_with_connection() {
    use crate::mtp::virtual_device::{setup_virtual_mtp_device, virtual_device_test_lock};

    // Register a virtual device backed by a tmp dir.
    let _guard = virtual_device_test_lock().lock().await;
    let fixture = setup_virtual_mtp_device();
    let location_id = fixture.location_id;
    let device_id = crate::mtp::list_mtp_devices()
        .into_iter()
        .find(|d| d.location_id == location_id)
        .map(|d| d.id)
        .expect("the virtual device must appear in discovery");

    // Before connect: false.
    let vol = MtpVolume::new(&device_id, 65537, "Test");
    assert!(!vol.listing_is_watched(Path::new("/")), "expected false before connect");

    // Connect, then assert true.
    let info = connection_manager()
        .connect(&device_id, None)
        .await
        .expect("virtual-mtp connect should succeed");
    // Use whatever storage_id the virtual device reported (we don't care
    // which storage; the gate is volume-level).
    let storage_id = info.storages.first().expect("virtual device should have storages").id;
    let vol = MtpVolume::new(&device_id, storage_id, "Test");
    assert!(vol.listing_is_watched(Path::new("/")), "expected true once connected");

    // Disconnect, then assert false again.
    connection_manager()
        .disconnect(&device_id, None, crate::mtp::connection::MtpDisconnectReason::User)
        .await
        .expect("virtual-mtp disconnect should succeed");
    assert!(
        !vol.listing_is_watched(Path::new("/")),
        "expected false after disconnect"
    );
}

/// Source stream that yields one good chunk, then errors. Drives the
/// upload's data phase far enough that `SendObjectInfo` has created the
/// object on the device, then fails the transfer mid-stream. The library
/// surfaces the created object as `UploadError.partial`; cmdr must
/// best-effort delete it so no corrupt artifact lingers on the device.
#[cfg(feature = "virtual-mtp")]
struct ErroringStream {
    emitted: bool,
}

#[cfg(feature = "virtual-mtp")]
impl futures_util::Stream for ErroringStream {
    type Item = Result<bytes::Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<Self::Item>> {
        if self.emitted {
            std::task::Poll::Ready(Some(Err(std::io::Error::other("simulated mid-stream read failure"))))
        } else {
            self.emitted = true;
            std::task::Poll::Ready(Some(Ok(bytes::Bytes::from_static(b"partial-bytes"))))
        }
    }
}

/// Connects the virtual device, starts an upload whose source stream errors
/// mid-transfer, and asserts the destination object does NOT exist on the
/// device afterward (cmdr deleted the partial via `UploadError.partial`).
#[cfg(feature = "virtual-mtp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upload_failure_deletes_partial_object_on_device() {
    use crate::mtp::virtual_device::{setup_virtual_mtp_device, virtual_device_test_lock};

    let _guard = virtual_device_test_lock().lock().await;
    let fixture = setup_virtual_mtp_device();
    let location_id = fixture.location_id;
    let device_id = crate::mtp::list_mtp_devices()
        .into_iter()
        .find(|d| d.location_id == location_id)
        .map(|d| d.id)
        .expect("the virtual device must appear in discovery");

    let info = connection_manager()
        .connect(&device_id, None)
        .await
        .expect("virtual-mtp connect should succeed");
    let storage_id = info.storages.first().expect("virtual device should have storages").id;

    // Prime the path cache so the upload can resolve "Documents" to a handle.
    connection_manager()
        .list_directory(&device_id, storage_id, "/")
        .await
        .expect("list root should succeed");

    let filename = "will-fail.txt";
    // Declared size is larger than the single emitted chunk, so the data
    // phase keeps pulling and hits the error after the object already
    // exists on the device.
    let size = 4096;
    let stream = Box::pin(ErroringStream { emitted: false });

    let result = connection_manager()
        .upload_from_stream(&device_id, storage_id, "Documents", filename, size, stream)
        .await;

    assert!(result.is_err(), "upload with a mid-stream source error must fail");

    // The partial object must be gone: a fresh listing of /Documents must
    // not contain the destination name.
    let entries = connection_manager()
        .list_directory(&device_id, storage_id, "/Documents")
        .await
        .expect("list Documents should succeed");
    assert!(
        !entries.iter().any(|e| e.name == filename),
        "partial object {filename} must not linger on the device after a failed upload; \
         found entries: {:?}",
        entries.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    connection_manager()
        .disconnect(&device_id, None, crate::mtp::connection::MtpDisconnectReason::User)
        .await
        .expect("virtual-mtp disconnect should succeed");
}

/// Like the error test, but the source stream signals cancellation
/// (`io::ErrorKind::Interrupted` — exactly what the cancel adapter in
/// `volume_read_stream_to_chunk_stream` produces on `ControlFlow::Break`).
/// Asserts two things: (1) the partial is still deleted (the user
/// cancelled — don't leave a half-file on their phone), and (2) the error
/// still surfaces as `Cancelled`, not a generic error, so the write-op
/// layer classifies it as a cancel.
#[cfg(feature = "virtual-mtp")]
struct CancellingStream {
    emitted: bool,
}

#[cfg(feature = "virtual-mtp")]
impl futures_util::Stream for CancellingStream {
    type Item = Result<bytes::Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<Self::Item>> {
        if self.emitted {
            std::task::Poll::Ready(Some(Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Operation cancelled",
            ))))
        } else {
            self.emitted = true;
            std::task::Poll::Ready(Some(Ok(bytes::Bytes::from_static(b"partial-bytes"))))
        }
    }
}

#[cfg(feature = "virtual-mtp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upload_cancel_deletes_partial_and_surfaces_cancelled() {
    use crate::mtp::virtual_device::{setup_virtual_mtp_device, virtual_device_test_lock};

    let _guard = virtual_device_test_lock().lock().await;
    let fixture = setup_virtual_mtp_device();
    let location_id = fixture.location_id;
    let device_id = crate::mtp::list_mtp_devices()
        .into_iter()
        .find(|d| d.location_id == location_id)
        .map(|d| d.id)
        .expect("the virtual device must appear in discovery");

    let info = connection_manager()
        .connect(&device_id, None)
        .await
        .expect("virtual-mtp connect should succeed");
    let storage_id = info.storages.first().expect("virtual device should have storages").id;

    connection_manager()
        .list_directory(&device_id, storage_id, "/")
        .await
        .expect("list root should succeed");

    let filename = "cancelled.txt";
    let size = 4096;
    let stream = Box::pin(CancellingStream { emitted: false });

    let result = connection_manager()
        .upload_from_stream(&device_id, storage_id, "Documents", filename, size, stream)
        .await;

    // Cancel classification preserved: the error must be Cancelled, not a
    // generic Other/Protocol error.
    assert!(
        matches!(result, Err(MtpConnectionError::Cancelled { .. })),
        "a cancelled upload must surface as MtpConnectionError::Cancelled, got: {result:?}"
    );

    // Partial deleted on cancel too.
    let entries = connection_manager()
        .list_directory(&device_id, storage_id, "/Documents")
        .await
        .expect("list Documents should succeed");
    assert!(
        !entries.iter().any(|e| e.name == filename),
        "partial object {filename} must not linger on the device after a cancelled upload; \
         found entries: {:?}",
        entries.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    connection_manager()
        .disconnect(&device_id, None, crate::mtp::connection::MtpDisconnectReason::User)
        .await
        .expect("virtual-mtp disconnect should succeed");
}

/// Source stream that yields the whole payload in one chunk, then ends.
/// A successful upload's data phase, for the stale-handle recovery test.
#[cfg(feature = "virtual-mtp")]
struct OneShotStream {
    chunk: Option<bytes::Bytes>,
}

#[cfg(feature = "virtual-mtp")]
impl futures_util::Stream for OneShotStream {
    type Item = Result<bytes::Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<Self::Item>> {
        std::task::Poll::Ready(self.chunk.take().map(Ok))
    }
}

/// A cached destination-folder handle that the device has since re-keyed
/// (Android MediaProvider rescanning between listing and upload) must NOT
/// fail the copy. The upload detects the `InvalidParentObject` rejection of
/// `SendObjectInfo`, refreshes the folder's handle, and signals
/// `StaleParentHandle`; the engine's one-shot retry (simulated here by a
/// second `upload_from_stream` with a fresh stream) then lands the file
/// against the refreshed handle.
///
/// Pre-fix this would have surfaced as a raw `ObjectNotFound` (rendered to
/// the user as a "Path not found" on the intact SOURCE file) with no retry.
#[cfg(feature = "virtual-mtp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upload_into_stale_parent_handle_heals_and_retry_succeeds() {
    use crate::mtp::virtual_device::{VIRTUAL_DEVICE_SERIAL, setup_virtual_mtp_device, virtual_device_test_lock};

    let _guard = virtual_device_test_lock().lock().await;
    let fixture = setup_virtual_mtp_device();
    let location_id = fixture.location_id;
    let device_id = crate::mtp::list_mtp_devices()
        .into_iter()
        .find(|d| d.location_id == location_id)
        .map(|d| d.id)
        .expect("the virtual device must appear in discovery");

    let info = connection_manager()
        .connect(&device_id, None)
        .await
        .expect("virtual-mtp connect should succeed");
    let storage_id = info.storages.first().expect("virtual device should have storages").id;

    // Browse so cmdr caches the (real, valid) handle for /Documents.
    connection_manager()
        .list_directory(&device_id, storage_id, "/")
        .await
        .expect("list root should succeed");

    // The device re-keys /Documents out from under cmdr — exactly what
    // Android's MediaProvider does across a media rescan. cmdr's cached handle
    // is now stale, so the next `SendObjectInfo` into it returns
    // `InvalidParentObject` (the field report). This drives the REAL device
    // behavior via mtp-rs, not a poke at cmdr's own cache.
    mtp_rs::rekey_virtual_object(VIRTUAL_DEVICE_SERIAL, Path::new("Documents"))
        .expect("/Documents was listed, so it must be re-keyable");

    let filename = "healed.txt";
    let payload = bytes::Bytes::from_static(b"contents that should land after the handle heals");
    let size = payload.len() as u64;

    // First attempt: the stale handle is rejected; the backend refreshes the
    // cache and signals a retry rather than a hard not-found.
    let first = connection_manager()
        .upload_from_stream(
            &device_id,
            storage_id,
            "Documents",
            filename,
            size,
            Box::pin(OneShotStream {
                chunk: Some(payload.clone()),
            }),
        )
        .await;
    assert!(
        matches!(first, Err(MtpConnectionError::StaleParentHandle { .. })),
        "a stale cached parent handle must signal StaleParentHandle (retryable), got: {first:?}"
    );

    // Retry with a fresh stream (what `stream_pipe_file` does): the refreshed
    // handle now resolves, so the file lands.
    let second = connection_manager()
        .upload_from_stream(
            &device_id,
            storage_id,
            "Documents",
            filename,
            size,
            Box::pin(OneShotStream { chunk: Some(payload) }),
        )
        .await;
    assert!(
        second.is_ok(),
        "the retry after the handle heals must succeed, got: {second:?}"
    );

    // The file is really on the device now.
    let entries = connection_manager()
        .list_directory(&device_id, storage_id, "/Documents")
        .await
        .expect("list Documents should succeed");
    assert!(
        entries.iter().any(|e| e.name == filename),
        "the healed upload must leave {filename} in /Documents; found: {:?}",
        entries.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    connection_manager()
        .disconnect(&device_id, None, crate::mtp::connection::MtpDisconnectReason::User)
        .await
        .expect("virtual-mtp disconnect should succeed");
}

/// End-to-end over the real wire: a multi-window file read through
/// `MtpVolume::open_read_stream` (the SHARED read path used by both the copy
/// and native drag-out) reassembles to the exact source bytes. Drives
/// repeated `GetPartialObject64` at advancing offsets via the virtual
/// transport, with the window shrunk so a small fixture spans several windows.
/// This is the bounded-window analogue of a copy's source read; the offset
/// accounting itself is unit-tested above with a scripted reader.
#[cfg(feature = "virtual-mtp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bounded_window_read_assembles_byte_exact() {
    use crate::mtp::virtual_device::{rescan_virtual_device, setup_virtual_mtp_device, virtual_device_test_lock};

    let _guard = virtual_device_test_lock().lock().await;
    let fixture = setup_virtual_mtp_device();
    let location_id = fixture.location_id;
    // Derive the device id the way discovery does (serial-based when the
    // device reports one), not `format!("mtp-{location_id}")`.
    let device_id = crate::mtp::list_mtp_devices()
        .into_iter()
        .find(|d| d.location_id == location_id)
        .map(|d| d.id)
        .expect("the virtual device must appear in discovery");

    // Write a fixture larger than one (shrunk) window into the internal
    // backing dir, then rescan so the virtual device hands it a handle.
    let payload: Vec<u8> = (0..3500u32).map(|i| (i % 251) as u8).collect();
    let internal = fixture.root().join("internal");
    std::fs::write(internal.join("bigfile.bin"), &payload).expect("write fixture");
    rescan_virtual_device();

    // 1000-byte windows over 3500 bytes ⇒ 4 windows (1000, 1000, 1000, 500).
    test_window::set(1000);

    let info = connection_manager()
        .connect(&device_id, None)
        .await
        .expect("virtual-mtp connect should succeed");
    let storage_id = info.storages.first().expect("virtual device should have storages").id;

    // Prime the path cache (resolve_path_to_handle is cache-only).
    connection_manager()
        .list_directory(&device_id, storage_id, "/")
        .await
        .expect("list root should succeed");

    let vol = MtpVolume::new(&device_id, storage_id, "Internal Storage");
    let mut stream = vol
        .open_read_stream(Path::new("/bigfile.bin"))
        .await
        .expect("open_read_stream should succeed");

    assert_eq!(stream.total_size(), payload.len() as u64);

    let mut assembled = Vec::new();
    let mut windows = 0;
    while let Some(item) = stream.next_chunk().await {
        let chunk = item.expect("each window read should be Ok");
        assert!(!chunk.is_empty(), "a window before EOF must not be empty");
        windows += 1;
        assembled.extend_from_slice(&chunk);
    }

    assert_eq!(
        assembled, payload,
        "bounded windows reassemble to the exact source bytes"
    );
    assert_eq!(stream.bytes_read(), payload.len() as u64);
    assert!(
        windows >= 2,
        "the fixture must span multiple bounded windows (got {windows}); else this isn't testing windowing"
    );

    // Cancel-keeps-partials, on the same fixture (one test owns the
    // `test_window` global, so there's no cross-test race on it). Open a
    // fresh stream, read ONE window, then `cancel_and_release`: it holds
    // nothing between windows, so it returns without a device drain, and the
    // bytes already delivered (the kept partial) survive in `bytes_read`.
    // Dropping afterward must not panic. This is Cmdr's stream contract — the
    // window bookkeeping is mtp-rs's, but "a cancel mid-read keeps the
    // partial" is what the copy engine relies on.
    let mut partial = vol
        .open_read_stream(Path::new("/bigfile.bin"))
        .await
        .expect("open_read_stream should succeed");
    let first = partial.next_chunk().await.expect("a window").expect("ok");
    assert_eq!(first.len(), 1000, "the first window is one full (shrunk) window");
    assert_eq!(partial.bytes_read(), 1000, "offset advanced by the window length");
    partial.cancel_and_release().await;
    assert_eq!(partial.bytes_read(), 1000, "the kept partial offset survives a cancel");
    drop(partial); // no panic, nothing held

    test_window::set(0);
    connection_manager()
        .disconnect(&device_id, None, crate::mtp::connection::MtpDisconnectReason::User)
        .await
        .expect("virtual-mtp disconnect should succeed");
}

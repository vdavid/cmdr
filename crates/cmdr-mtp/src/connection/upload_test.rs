//! What an upload does to the device when its source stops early, and when the
//! destination folder's handle went stale underneath it.
//!
//! All three cells are about the SESSION layer's upload rather than the volume's:
//! they drive `upload_from_stream` directly, and what they assert on is the
//! object the device is left holding. The volume's own byte path is
//! `volume::read_range_test` and `volume::streams_test`.
//!
//! A partial object is the shape of the risk here. `SendObjectInfo` creates the
//! object on the phone BEFORE any byte of it arrives, so every way the data phase
//! can end early leaves a corrupt file in the user's gallery unless the backend
//! cleans it up.

use std::path::Path;
use std::pin::Pin;

use crate::connection::MtpConnectionError;
use crate::testing::{connect_virtual_device, device_lock, test_connection_manager};
use crate::virtual_device::VIRTUAL_DEVICE_SERIAL;

/// Source stream that yields one good chunk, then errors. Drives the
/// upload's data phase far enough that `SendObjectInfo` has created the
/// object on the device, then fails the transfer mid-stream. The library
/// surfaces the created object as `UploadError.partial`; cmdr must
/// best-effort delete it so no corrupt artifact lingers on the device.
struct ErroringStream {
    emitted: bool,
}

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
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upload_failure_deletes_partial_object_on_device() {
    let _guard = device_lock().await;
    let device = connect_virtual_device(test_connection_manager()).await;

    let filename = "will-fail.txt";
    // Declared size is larger than the single emitted chunk, so the data
    // phase keeps pulling and hits the error after the object already
    // exists on the device.
    let size = 4096;
    let stream = Box::pin(ErroringStream { emitted: false });

    let result = test_connection_manager()
        .upload_from_stream(&device.id, device.storage_id, "Documents", filename, size, stream)
        .await;

    assert!(result.is_err(), "upload with a mid-stream source error must fail");

    // The partial object must be gone: a fresh listing of /Documents must
    // not contain the destination name.
    let entries = test_connection_manager()
        .list_directory(&device.id, device.storage_id, "/Documents")
        .await
        .expect("list Documents should succeed");
    assert!(
        !entries.iter().any(|e| e.name == filename),
        "partial object {filename} must not linger on the device after a failed upload; \
         found entries: {:?}",
        entries.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    device.teardown(test_connection_manager()).await;
}

/// Like the error test, but the source stream signals cancellation
/// (`io::ErrorKind::Interrupted` — exactly what the cancel adapter in
/// `volume_read_stream_to_chunk_stream` produces on `ControlFlow::Break`).
/// Asserts two things: (1) the partial is still deleted (the user
/// cancelled — don't leave a half-file on their phone), and (2) the error
/// still surfaces as `Cancelled`, not a generic error, so the write-op
/// layer classifies it as a cancel.
struct CancellingStream {
    emitted: bool,
}

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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upload_cancel_deletes_partial_and_surfaces_cancelled() {
    let _guard = device_lock().await;
    let device = connect_virtual_device(test_connection_manager()).await;

    let filename = "cancelled.txt";
    let size = 4096;
    let stream = Box::pin(CancellingStream { emitted: false });

    let result = test_connection_manager()
        .upload_from_stream(&device.id, device.storage_id, "Documents", filename, size, stream)
        .await;

    // Cancel classification preserved: the error must be Cancelled, not a
    // generic Other/Protocol error.
    assert!(
        matches!(result, Err(MtpConnectionError::Cancelled { .. })),
        "a cancelled upload must surface as MtpConnectionError::Cancelled, got: {result:?}"
    );

    // Partial deleted on cancel too.
    let entries = test_connection_manager()
        .list_directory(&device.id, device.storage_id, "/Documents")
        .await
        .expect("list Documents should succeed");
    assert!(
        !entries.iter().any(|e| e.name == filename),
        "partial object {filename} must not linger on the device after a cancelled upload; \
         found entries: {:?}",
        entries.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    device.teardown(test_connection_manager()).await;
}

/// Source stream that yields the whole payload in one chunk, then ends.
/// A successful upload's data phase, for the stale-handle recovery test.
struct OneShotStream {
    chunk: Option<bytes::Bytes>,
}

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
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upload_into_stale_parent_handle_heals_and_retry_succeeds() {
    let _guard = device_lock().await;
    let device = connect_virtual_device(test_connection_manager()).await;

    // The device re-keys /Documents out from under cmdr — exactly what
    // Android's MediaProvider does across a media rescan. cmdr's cached handle
    // is now stale, so the next `SendObjectInfo` into it returns
    // `InvalidParentObject` (the field report). This drives the REAL device
    // behavior via mtp-rs, not a poke at cmdr's own cache.
    crate::virtual_device::rekey_virtual_object(VIRTUAL_DEVICE_SERIAL, Path::new("Documents"))
        .expect("/Documents was listed, so it must be re-keyable");

    let filename = "healed.txt";
    let payload = bytes::Bytes::from_static(b"contents that should land after the handle heals");
    let size = payload.len() as u64;

    // First attempt: the stale handle is rejected; the backend refreshes the
    // cache and signals a retry rather than a hard not-found.
    let first = test_connection_manager()
        .upload_from_stream(
            &device.id,
            device.storage_id,
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
    let second = test_connection_manager()
        .upload_from_stream(
            &device.id,
            device.storage_id,
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
    let entries = test_connection_manager()
        .list_directory(&device.id, device.storage_id, "/Documents")
        .await
        .expect("list Documents should succeed");
    assert!(
        entries.iter().any(|e| e.name == filename),
        "the healed upload must leave {filename} in /Documents; found: {:?}",
        entries.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    device.teardown(test_connection_manager()).await;
}

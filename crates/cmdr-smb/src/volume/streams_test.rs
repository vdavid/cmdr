//! The consumer side of the channel-backed read stream, driven off a pre-seeded
//! channel rather than a real producer task, plus the single-shot write promise.
//! End-to-end streaming is `streaming_integration_test.rs`.

use super::*;
use crate::volume::test_support::*;

// These test the consumer side of the channel-backed read stream in isolation.
// End-to-end SMB streaming is covered by the Docker integration tests below
// (smb_integration_open_read_stream, smb_integration_export_streams).

/// Builds a `ChannelReadStream` off a pre-seeded channel, bypassing the real SMB
/// producer task. Returns the stream plus the cancel receiver side so tests can
/// assert that drop sends a cancel signal.
fn make_stream_from_chunks(
    chunks: Vec<Result<Vec<u8>, VolumeError>>,
    total_size: u64,
) -> (ChannelReadStream, tokio::sync::oneshot::Receiver<()>) {
    let (chunk_tx, chunk_rx) =
        tokio::sync::mpsc::channel::<Result<Vec<u8>, VolumeError>>(SMB_STREAM_CHANNEL_CAPACITY.max(chunks.len()));
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();

    for chunk in chunks {
        // blocking_send is fine in tests; we sized the channel to fit.
        chunk_tx.try_send(chunk).expect("channel has capacity in test setup");
    }
    // Drop chunk_tx so recv returns None after draining.
    drop(chunk_tx);

    (ChannelReadStream::new(chunk_rx, cancel_tx, total_size), cancel_rx)
}

#[tokio::test]
async fn smb_read_stream_empty_file() {
    let (mut stream, _cancel_rx) = make_stream_from_chunks(vec![], 0);
    assert_eq!(stream.total_size(), 0);
    assert_eq!(stream.bytes_read(), 0);
    assert!(stream.next_chunk().await.is_none());
}

#[tokio::test]
async fn smb_read_stream_yields_chunks_in_order() {
    let (mut stream, _cancel_rx) =
        make_stream_from_chunks(vec![Ok(vec![1u8; 100]), Ok(vec![2u8; 50]), Ok(vec![3u8; 30])], 180);
    assert_eq!(stream.total_size(), 180);

    let c1 = stream.next_chunk().await.unwrap().unwrap();
    assert_eq!(c1, vec![1u8; 100]);
    assert_eq!(stream.bytes_read(), 100);

    let c2 = stream.next_chunk().await.unwrap().unwrap();
    assert_eq!(c2, vec![2u8; 50]);
    assert_eq!(stream.bytes_read(), 150);

    let c3 = stream.next_chunk().await.unwrap().unwrap();
    assert_eq!(c3, vec![3u8; 30]);
    assert_eq!(stream.bytes_read(), 180);

    assert!(stream.next_chunk().await.is_none());
}

#[tokio::test]
async fn smb_read_stream_propagates_mid_stream_error() {
    let (mut stream, _cancel_rx) = make_stream_from_chunks(
        vec![
            Ok(vec![1u8; 10]),
            Err(VolumeError::DeviceDisconnected("simulated".to_string())),
        ],
        0,
    );

    let first = stream.next_chunk().await.unwrap().unwrap();
    assert_eq!(first, vec![1u8; 10]);
    assert_eq!(stream.bytes_read(), 10);

    let second = stream.next_chunk().await.unwrap();
    assert!(matches!(second, Err(VolumeError::DeviceDisconnected(_))));
    // bytes_read should not have advanced on the error
    assert_eq!(stream.bytes_read(), 10);
}

#[tokio::test]
async fn smb_read_stream_drop_sends_cancel() {
    let (stream, mut cancel_rx) = make_stream_from_chunks(vec![Ok(vec![1u8; 10])], 10);
    drop(stream);

    // The cancel oneshot should have been fired by Drop.
    match cancel_rx.try_recv() {
        Ok(()) => {}
        other => panic!("expected cancel signal, got {other:?}"),
    }
}

#[test]
fn smb_supports_streaming() {
    // SmbVolume should report streaming support so cross-volume copies
    // (MTP↔SMB) use the streaming path instead of NotSupported/temp files.
    let vol = make_test_volume();
    assert!(vol.supports_streaming());
}

/// The compound fast path's condition, which is also the transfer layer's
/// staging exemption. A write is one all-or-nothing frame when it has bytes and
/// they fit one SMB2 WRITE; the boundary is inclusive, and an empty file is NOT
/// single-shot (it has no WRITE to compound with, so it takes the streaming
/// writer's create+finish).
#[test]
fn only_a_write_that_fits_one_compound_frame_is_single_shot() {
    assert!(!fits_one_compound_write(65_536, 0), "an empty file has no WRITE");
    assert!(fits_one_compound_write(65_536, 1));
    assert!(fits_one_compound_write(65_536, 65_536), "the limit itself fits");
    assert!(
        !fits_one_compound_write(65_536, 65_537),
        "one byte over needs two WRITEs"
    );
    assert!(
        fits_one_compound_write(8 * 1024 * 1024, 65_537),
        "a server that negotiated a bigger WRITE takes bigger files in one shot"
    );
}

/// No live session means no promise: the transfer layer stages the write, as it
/// does for any backend without the guarantee. ❌ Never answer from the size
/// alone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_disconnected_share_promises_nothing_about_single_shot_writes() {
    let vol = make_test_volume();
    assert!(!vol.write_is_single_shot(10).await);
}

//! The consumer side of the channel-backed read stream, driven off a pre-seeded
//! channel rather than a real producer task, plus the compound read's condition.
//! End-to-end streaming is `read_stream_integration_test.rs`; the frames it
//! costs are `wire_shape_integration_test.rs`.

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

    (
        ChannelReadStream::new(chunk_rx, cancel_tx, StreamLength::Known(total_size)),
        cancel_rx,
    )
}

#[tokio::test]
async fn smb_read_stream_empty_file() {
    let (mut stream, _cancel_rx) = make_stream_from_chunks(vec![], 0);
    assert_eq!(stream.total_size(), StreamLength::Known(0));
    assert_eq!(stream.bytes_read(), 0);
    assert!(stream.next_chunk().await.is_none());
}

#[tokio::test]
async fn smb_read_stream_yields_chunks_in_order() {
    let (mut stream, _cancel_rx) =
        make_stream_from_chunks(vec![Ok(vec![1u8; 100]), Ok(vec![2u8; 50]), Ok(vec![3u8; 30])], 180);
    assert_eq!(stream.total_size(), StreamLength::Known(180));

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
            Err(VolumeError::DeviceDisconnected("simulated".into())),
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

/// The hinted read's compound path follows the limit it's handed, which is
/// `Connection::quick_read_limit()`: one download chunk on a cold connection,
/// what the link moves in 250 ms once a download has measured it. The one-chunk
/// floor and the `MaxReadSize` cap are smb2's arithmetic, tested there. The
/// boundary is inclusive, and an empty file has nothing to read.
#[test]
fn a_read_takes_the_compound_path_up_to_the_quick_read_limit() {
    let chunk = smb2::DOWNLOAD_CHUNK_SIZE as u64;
    let measured = 4_000_000; // 16 MB/s × 250 ms
    assert!(!fits_one_compound_read(chunk, 0), "an empty file has no READ");
    assert!(fits_one_compound_read(chunk, 1));
    assert!(fits_one_compound_read(chunk, chunk), "the limit itself fits");
    assert!(
        !fits_one_compound_read(chunk, chunk + 1),
        "a cold connection streams anything over one chunk"
    );
    assert!(
        fits_one_compound_read(measured, chunk + 1),
        "a link measured fast enough takes a multi-chunk file in one READ"
    );
    assert!(fits_one_compound_read(measured, measured));
    assert!(
        !fits_one_compound_read(measured, measured + 1),
        "one byte past the limit streams"
    );
}

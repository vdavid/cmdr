//! Cutting a stream into parts, and how often a failed part is tried again.

use std::pin::Pin;
use std::time::Duration;

use cmdr_fs::volume::{StreamLength, VolumeError, VolumeReadStream};

use super::{PartReader, retry_after};

struct Pieces(Vec<Vec<u8>>);

impl VolumeReadStream for Pieces {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move { (!self.0.is_empty()).then(|| Ok(self.0.remove(0))) })
    }

    fn total_size(&self) -> StreamLength {
        StreamLength::Unknown
    }

    fn bytes_read(&self) -> u64 {
        0
    }

    fn modified_at(&self) -> Option<std::time::SystemTime> {
        None
    }
}

/// Pieces numbered by their position in the stream, so a part that took the
/// wrong bytes shows it.
fn numbered(sizes: &[usize]) -> (Box<dyn VolumeReadStream>, Vec<u8>) {
    let mut all = Vec::new();
    let mut pieces = Vec::new();
    for &size in sizes {
        let piece: Vec<u8> = (0..size).map(|i| ((all.len() + i) % 251) as u8).collect();
        all.extend_from_slice(&piece);
        pieces.push(piece);
    }
    (Box::new(Pieces(pieces)), all)
}

#[tokio::test]
async fn parts_take_exactly_their_size_across_piece_boundaries() {
    let (stream, all) = numbered(&[3, 4, 5]);
    let mut reader = PartReader::new(stream);
    let first = reader.fill(5).await.expect("a part");
    let second = reader.fill(5).await.expect("a part");
    let last = reader.fill(5).await.expect("the tail");
    assert_eq!(first, all[0..5]);
    assert_eq!(second, all[5..10]);
    assert_eq!(last, all[10..12], "the last part is whatever is left");
    assert!(reader.at_end().await.expect("an answer"));
}

#[tokio::test]
async fn a_source_that_ends_on_a_part_boundary_has_no_empty_part() {
    let (stream, _) = numbered(&[5, 5]);
    let mut reader = PartReader::new(stream);
    assert_eq!(reader.fill(5).await.expect("a part").len(), 5);
    assert_eq!(reader.fill(5).await.expect("a part").len(), 5);
    assert!(reader.fill(5).await.expect("the end").is_empty());
}

/// The check a known-length upload makes after its last part: anything still
/// in the source means it lied about its length, and the upload is refused.
#[tokio::test]
async fn bytes_past_the_last_part_are_noticed() {
    let (stream, all) = numbered(&[6, 2]);
    let mut reader = PartReader::new(stream);
    assert_eq!(reader.fill(6).await.expect("a part"), all[0..6]);
    assert!(!reader.at_end().await.expect("an answer"));
    // The look ahead lost nothing.
    assert_eq!(reader.fill(6).await.expect("the rest"), all[6..8]);
}

/// Hands out its pieces, then never answers again: a source that stalls right
/// at its end, where only the look-ahead past the last part is waiting.
struct StallsAtEnd(Vec<Vec<u8>>);

impl VolumeReadStream for StallsAtEnd {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.0.is_empty() {
                std::future::pending::<()>().await;
            }
            Some(Ok(self.0.remove(0)))
        })
    }

    fn total_size(&self) -> StreamLength {
        StreamLength::Unknown
    }

    fn bytes_read(&self) -> u64 {
        0
    }

    fn modified_at(&self) -> Option<std::time::SystemTime> {
        None
    }
}

/// A Cancel while the end-of-source check waits on a stalled source is heard
/// on the next progress tick, as it is while a part fills.
#[tokio::test(start_paused = true)]
async fn a_cancel_while_the_end_check_waits_on_a_stalled_source_returns() {
    let mut reader = PartReader::new(Box::new(StallsAtEnd(vec![vec![7; 4]])));
    assert_eq!(reader.fill(4).await.expect("a part").len(), 4);
    let mut between = || Err(VolumeError::Cancelled("s3-test".to_string()));
    let checked = tokio::time::timeout(Duration::from_secs(2), reader.at_end_with(&mut between)).await;
    let answer = checked.expect("Cancel returns within a couple of progress ticks");
    assert!(matches!(answer, Err(VolumeError::Cancelled(_))), "got {answer:?}");
}

#[test]
fn a_failed_part_is_tried_three_more_times_with_growing_waits() {
    assert_eq!(retry_after(1), Some(Duration::from_secs(1)));
    assert_eq!(retry_after(2), Some(Duration::from_secs(2)));
    assert_eq!(retry_after(3), Some(Duration::from_secs(4)));
    assert_eq!(retry_after(4), None);
}

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::super::InMemoryVolume;
use super::*;
use crate::entry::FileEntry;
use crate::volume::{Volume, VolumeError, WriteMode};

const MIB: u64 = 1024 * 1024;

/// A volume holding one synthetic file at `/photo.jpg`, plus `/out`.
fn device_with_photo(len: u64) -> InMemoryVolume {
    InMemoryVolume::with_entries(
        "device",
        vec![
            FileEntry::new("photo.jpg".into(), "/photo.jpg".into(), false, false),
            FileEntry::new("out".into(), "/out".into(), true, false),
        ],
    )
    .with_content("/photo.jpg", InMemoryContent::synthetic(42, len))
}

/// Streams `from` on `source` into `to` on `dest`, the way the copy engine does.
async fn stream_copy(source: &InMemoryVolume, from: &str, dest: &InMemoryVolume, to: &str) -> Result<u64, VolumeError> {
    let stream = source.open_read_stream(Path::new(from)).await?;
    let length = stream.total_size();
    let written = dest
        .write_from_stream(Path::new(to), WriteMode::CreateNew, length, stream, &|_| {
            std::ops::ControlFlow::Continue(())
        })
        .await?;
    Ok(written.bytes)
}

fn content_of(volume: &InMemoryVolume, path: &str) -> InMemoryContent {
    volume.entries.read_ignore_poison()[&PathBuf::from(path)]
        .content
        .clone()
        .expect("a file holds content")
}

#[tokio::test]
async fn a_streamed_copy_of_a_synthetic_file_stays_a_descriptor_and_reads_back_identical() {
    let source = device_with_photo(3 * MIB + 17);
    let dest = InMemoryVolume::new("dest");
    assert_eq!(
        stream_copy(&source, "/photo.jpg", &dest, "/copy.jpg").await.unwrap(),
        3 * MIB + 17
    );

    assert_eq!(
        content_of(&dest, "/copy.jpg"),
        InMemoryContent::synthetic(42, 3 * MIB + 17)
    );
    let size = dest.get_metadata(Path::new("/copy.jpg")).await.unwrap().size;
    assert_eq!(size, Some(3 * MIB + 17));
    let window = dest.read_range(Path::new("/copy.jpg"), MIB - 3, 100).await.unwrap();
    let original = source.read_range(Path::new("/photo.jpg"), MIB - 3, 100).await.unwrap();
    assert_eq!(window, original);
}

#[tokio::test]
async fn a_listing_only_file_still_streams_as_empty() {
    let volume = InMemoryVolume::with_entries("plain", vec![FileEntry::new("a".into(), "/a".into(), false, false)]);
    let mut stream = volume.open_read_stream(Path::new("/a")).await.unwrap();
    assert!(stream.next_chunk().await.is_none());
}

#[tokio::test]
async fn capacity_answers_free_space_live_and_refuses_a_write_that_wont_fit() {
    let source = device_with_photo(4 * MIB);
    let dest = device_with_photo(4 * MIB).with_capacity(10 * MIB);
    let free = |volume: &InMemoryVolume| match futures_util::FutureExt::now_or_never(volume.get_space_info()) {
        Some(Ok(info)) => info.available_bytes(),
        other => panic!("no space answer: {:?}", other.map(|r| r.map(|_| ()))),
    };
    assert_eq!(free(&dest), Some(6 * MIB));
    stream_copy(&source, "/photo.jpg", &dest, "/out/a.jpg").await.unwrap();
    assert_eq!(free(&dest), Some(2 * MIB));

    let refused = stream_copy(&source, "/photo.jpg", &dest, "/out/b.jpg").await;
    assert!(matches!(refused, Err(VolumeError::StorageFull { .. })), "{refused:?}");
    assert!(!dest.exists(Path::new("/out/b.jpg")).await, "nothing landed");
}

#[tokio::test(start_paused = true)]
async fn a_slow_device_paces_streams_but_never_positioned_reads() {
    let volume = device_with_photo(2 * MIB).with_bandwidth(MIB);
    let started = rt::time::now();
    volume
        .read_range(Path::new("/photo.jpg"), 0, MIB as usize)
        .await
        .unwrap();
    assert_eq!(rt::time::now(), started, "a positioned read is memory-fast");

    let dest = InMemoryVolume::new("dest");
    stream_copy(&volume, "/photo.jpg", &dest, "/copy.jpg").await.unwrap();
    assert_took(rt::time::now() - started, Duration::from_secs(2));
}

#[tokio::test]
async fn a_yielding_device_hands_the_thread_back_at_a_call() {
    let volume = device_with_photo(10).with_yielding(Duration::ZERO);
    let listing = volume.list_directory(Path::new("/"), None);
    assert!(
        futures_util::FutureExt::now_or_never(listing).is_none(),
        "a spent slice yields before answering"
    );
    let quiet = device_with_photo(10);
    assert!(futures_util::FutureExt::now_or_never(quiet.list_directory(Path::new("/"), None)).is_some());
}

#[test]
fn unmix_inverts_mix() {
    for z in [0u64, 1, 42, u64::MAX, 0x0123_4567_89AB_CDEF, GOLDEN] {
        assert_eq!(unmix(mix(z)), z, "{z:#x}");
    }
}

#[test]
fn generated_bytes_are_a_pure_function_of_seed_and_offset() {
    let whole = generated(7, 0, 1_000);
    assert_eq!(whole.len(), 1_000);
    // Any window, at any alignment, is the same bytes the whole read had there.
    for (offset, len) in [(0, 1), (3, 5), (8, 8), (13, 200), (999, 1)] {
        assert_eq!(generated(7, offset, len), whole[offset as usize..offset as usize + len]);
    }
    assert_ne!(generated(8, 0, 64), generated(7, 0, 64), "seeds differ");
}

#[test]
fn synthetic_content_reads_its_stated_size_and_stops_there() {
    let content = InMemoryContent::synthetic(3, 100);
    assert_eq!(content.len(), 100);
    assert_eq!(content.read_at(90, 64).len(), 10);
    assert!(content.read_at(100, 8).is_empty());
    assert_eq!(content.read_at(10, 20), content.read_at(0, 100)[10..30]);
}

#[test]
fn a_write_of_generated_bytes_keeps_the_descriptor() {
    let source = InMemoryContent::synthetic(0xDEAD_BEEF, 200_000);
    let mut sink = ContentSink::default();
    // Chunks of an odd size, so the probe and the checks straddle words.
    let mut at = 0;
    while at < source.len() {
        let chunk = source.read_at(at, 65_537);
        at += chunk.len() as u64;
        sink.push(&chunk);
    }
    assert_eq!(sink.finish(), source);
}

#[test]
fn one_changed_byte_keeps_real_bytes_from_the_start() {
    let source = InMemoryContent::synthetic(11, 300);
    let mut edited = source.read_at(0, 300);
    edited[250] ^= 1;
    let mut sink = ContentSink::default();
    sink.push(&edited[..128]);
    sink.push(&edited[128..]);
    assert_eq!(sink.finish(), InMemoryContent::bytes(edited));
}

#[test]
fn real_bytes_and_tiny_files_stay_bytes() {
    for data in [b"hello world, plain text".to_vec(), b"tiny".to_vec(), Vec::new()] {
        let mut sink = ContentSink::default();
        sink.push(&data);
        assert_eq!(sink.finish(), InMemoryContent::bytes(data));
    }
}

#[tokio::test(start_paused = true)]
async fn bandwidth_is_one_budget_the_whole_device_shares() {
    let device = Arc::new(Bandwidth::new(1_000_000));
    let started = rt::time::now();
    device.transfer(1_000_000).await;
    assert_took(rt::time::now() - started, Duration::from_secs(1));

    // Three readers at once split it: all three are done after three seconds, not one.
    let started = rt::time::now();
    let readers: Vec<_> = (0..3)
        .map(|_| {
            let device = Arc::clone(&device);
            tokio::spawn(async move { device.transfer(1_000_000).await })
        })
        .collect();
    for reader in readers {
        reader.await.expect("the reader ends");
    }
    assert_took(rt::time::now() - started, Duration::from_secs(3));
}

#[tokio::test(start_paused = true)]
async fn a_dropped_transfer_returns_at_once() {
    let device = Bandwidth::new(1_000);
    let wait = rt::time::timeout(Duration::from_millis(10), device.transfer(1_000_000)).await;
    assert!(
        wait.is_err(),
        "the transfer is still waiting, and dropping it is the cancel"
    );
}

#[tokio::test]
async fn a_yielder_yields_only_once_its_slice_is_spent() {
    let yielder = Yielder::new(Duration::from_secs(3_600));
    // A fresh slice: no yield, so the future is ready on its first poll.
    assert!(futures_util::FutureExt::now_or_never(yielder.breathe()).is_some());
    let spent = Yielder::new(Duration::ZERO);
    assert!(
        futures_util::FutureExt::now_or_never(spent.breathe()).is_none(),
        "a spent slice yields (pends once)"
    );
}

/// `took` is `expected`, give or take the millisecond tokio's timer rounds a
/// final wake-up to.
fn assert_took(took: Duration, expected: Duration) {
    assert!(
        took >= expected && took <= expected + Duration::from_millis(2),
        "took {took:?}, expected {expected:?}"
    );
}

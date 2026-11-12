//! What `write_from_stream` makes durable, and what it leaves to the caller's
//! `flush_writes`: each file handed to the drive as it lands, one device barrier
//! for all of them at the close (#1145).

use super::*;
use cmdr_fs::testing::TestDir;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{InMemoryVolume, Volume, WriteMode};
use std::path::Path;

#[tokio::test]
async fn test_write_from_stream_multichunk_is_durable_and_correct() {
    use std::fs;

    // A multi-chunk payload (> InMemory's 64 KiB chunk) exercises the chunk
    // loop and the finish (`sync_to_drive` + the barrier notes) that lands
    // after it. We can't spy on `fsync` from a unit test, so this pins the
    // regression that matters: the function returns normally and the file is
    // fully + correctly written through the finish path. The barrier count is
    // `write_from_stream_owes_one_barrier_per_device_not_per_file` below.
    let vol_dir = TestDir::new("write_from_stream_durable_test");

    // 200 KiB of deterministic bytes => ~4 chunks at 64 KiB each.
    let payload: Vec<u8> = (0..200 * 1024).map(|i| (i % 251) as u8).collect();

    let source = InMemoryVolume::new("Source");
    source.create_file(Path::new("/big.bin"), &payload).await.unwrap();

    let stream = source.open_read_stream(Path::new("/big.bin")).await.unwrap();
    let size = stream.total_size();
    let volume = LocalPosixVolume::local_folder("Test", vol_dir.to_str().unwrap(), VolumeHost::detached());
    let bytes = volume
        .write_from_stream(
            Path::new("imported.bin"),
            WriteMode::CreateOrReplace,
            size,
            stream,
            &|_| std::ops::ControlFlow::Continue(()),
        )
        .await
        .unwrap();

    assert_eq!(bytes.bytes, payload.len() as u64);
    assert_eq!(fs::read(vol_dir.join("imported.bin")).unwrap(), payload);
}

/// #1145: streaming many small files in costs no device barrier per file; the
/// caller's `flush_writes` pays one for the device they all landed on (none on
/// Linux, whose `fdatasync` already flushed the drive's cache). The interval is
/// pushed out of reach so only the close counts.
#[tokio::test]
async fn write_from_stream_owes_one_barrier_per_device_not_per_file() {
    const FILES: usize = 50;
    let vol_dir = TestDir::new("write_from_stream_barriers");
    let source = InMemoryVolume::new("Source");
    let volume = LocalPosixVolume::local_folder("Test", vol_dir.to_str().unwrap(), VolumeHost::detached())
        .with_barrier_interval(std::time::Duration::MAX);
    for i in 0..FILES {
        let name = format!("/f{i}.txt");
        source.create_file(Path::new(&name), b"small").await.unwrap();
        let stream = source.open_read_stream(Path::new(&name)).await.unwrap();
        let size = stream.total_size();
        volume
            .write_from_stream(Path::new(&name[1..]), WriteMode::CreateNew, size, stream, &|_| {
                std::ops::ControlFlow::Continue(())
            })
            .await
            .unwrap();
    }
    assert_eq!(volume.barriers_issued(), 0, "no barrier while the files stream in");

    volume.flush_writes().await.unwrap();
    let expected = if cfg!(target_os = "macos") { 1 } else { 0 };
    assert_eq!(volume.barriers_issued(), expected);
    assert_eq!(std::fs::read_dir(&*vol_dir).unwrap().count(), FILES);
}

//! Carrying a copy on after an outage on a local disk: a write a dropped source
//! cut short continues into the same file (`Volume::continue_write_from_stream`),
//! and a local source reads on from an offset on the file it first read
//! (`Volume::resume_read_stream`).

use super::*;
use cmdr_fs::testing::TestDir;
use cmdr_fs::volume::WriteMode;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{StreamLength, Volume, VolumeError};
use std::path::Path;

/// A source stream that fails, as a dropped share does, once it has handed out
/// `after` bytes.
struct BreaksAfter {
    inner: Box<dyn cmdr_fs::volume::VolumeReadStream>,
    after: u64,
    delivered: u64,
}

impl cmdr_fs::volume::VolumeReadStream for BreaksAfter {
    fn next_chunk(
        &mut self,
    ) -> std::pin::Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.delivered >= self.after {
                return Some(Err(VolumeError::DeviceDisconnected("the share went".into())));
            }
            let chunk = self.inner.next_chunk().await;
            if let Some(Ok(bytes)) = &chunk {
                self.delivered += bytes.len() as u64;
            }
            chunk
        })
    }
    fn total_size(&self) -> StreamLength {
        self.inner.total_size()
    }
    fn bytes_read(&self) -> u64 {
        self.delivered
    }
    fn modified_at(&self) -> Option<std::time::SystemTime> {
        self.inner.modified_at()
    }
}

/// A local disk carries a write a dropped source cut short on from the last
/// byte it reported, into the same file, and the file ends whole
/// (`Volume::continue_write_from_stream`).
#[tokio::test]
async fn a_write_a_dropped_source_cut_short_carries_on_from_its_last_reported_byte() {
    use cmdr_fs::volume::InMemoryVolume;
    use std::sync::atomic::{AtomicU64, Ordering};

    let dir = TestDir::new("continue_write");
    let data: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    let source = InMemoryVolume::new("Source");
    source.create_file(Path::new("/big.bin"), &data).await.unwrap();
    let volume = LocalPosixVolume::local_folder("Test", dir.to_str().unwrap(), VolumeHost::detached());
    assert!(volume.continues_writes());

    let first = source.open_read_stream(Path::new("/big.bin")).await.unwrap();
    let version = first.source_version().expect("an in-memory file names its version");
    let reported = AtomicU64::new(0);
    let failed = volume
        .write_from_stream(
            Path::new("big.bin.tmp"),
            WriteMode::CreateOrReplace,
            StreamLength::Known(data.len() as u64),
            Box::new(BreaksAfter {
                inner: first,
                after: 128 * 1024,
                delivered: 0,
            }),
            &|progress| {
                reported.store(progress.bytes_written, Ordering::SeqCst);
                std::ops::ControlFlow::Continue(())
            },
        )
        .await;
    assert!(matches!(failed, Err(VolumeError::DeviceDisconnected(_))));
    let at = reported.load(Ordering::SeqCst);
    assert!(at > 0, "the write reported bytes before the source broke");

    let rest = source
        .resume_read_stream(Path::new("/big.bin"), &version, at)
        .await
        .unwrap();
    let written = volume
        .continue_write_from_stream(
            Path::new("big.bin.tmp"),
            at,
            StreamLength::Known(data.len() as u64),
            rest,
            &|progress| {
                assert!(progress.bytes_written > at, "progress counts from the file's start");
                std::ops::ControlFlow::Continue(())
            },
        )
        .await
        .unwrap();
    assert_eq!(written.bytes, data.len() as u64);
    assert!(
        std::fs::read(dir.join("big.bin.tmp")).unwrap() == data,
        "whole, every byte once"
    );
}

/// A target that's gone, or holds fewer bytes than the write says it got, isn't
/// carried on: `NotSupported`, and the engine runs the file again.
#[tokio::test]
async fn a_write_is_not_carried_on_into_a_target_that_is_gone_or_short() {
    use cmdr_fs::volume::InMemoryVolume;

    let dir = TestDir::new("continue_write_refused");
    let source = InMemoryVolume::new("Source");
    source.create_file(Path::new("/f"), b"0123456789").await.unwrap();
    let volume = LocalPosixVolume::local_folder("Test", dir.to_str().unwrap(), VolumeHost::detached());
    std::fs::write(dir.join("short.tmp"), b"0123").unwrap();

    for (target, at) in [("gone.tmp", 4), ("short.tmp", 6)] {
        let rest = source.open_read_stream(Path::new("/f")).await.unwrap();
        let refused = volume
            .continue_write_from_stream(Path::new(target), at, StreamLength::Known(10), rest, &|_| {
                std::ops::ControlFlow::Continue(())
            })
            .await;
        assert!(
            matches!(refused, Err(VolumeError::NotSupported)),
            "{target}: expected NotSupported, got {refused:?}"
        );
    }
    assert!(!dir.join("gone.tmp").exists(), "nothing is created to carry on into");
}

/// A local source carries a read on from an offset on the file it first read
/// (`Volume::resume_read_stream`), and refuses one written over since, even
/// with the same bytes: its inode and change time moved.
#[tokio::test]
async fn a_local_read_resumes_on_its_named_file_and_refuses_a_rewritten_one() {
    let dir = TestDir::new("resume_read");
    let data: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(dir.join("f.bin"), &data).unwrap();
    let volume = LocalPosixVolume::local_folder("Test", dir.to_str().unwrap(), VolumeHost::detached());

    let first = volume.open_read_stream(Path::new("f.bin")).await.unwrap();
    let version = first.source_version().expect("a local file names its version");
    drop(first);
    let mut rest = volume
        .resume_read_stream(Path::new("f.bin"), &version, 70_000)
        .await
        .expect("the same file resumes");
    let mut read = Vec::new();
    while let Some(chunk) = rest.next_chunk().await {
        read.extend_from_slice(&chunk.unwrap());
    }
    assert!(read == data[70_000..], "exactly the bytes from the offset");

    std::fs::remove_file(dir.join("f.bin")).unwrap();
    std::fs::write(dir.join("f.bin"), &data).unwrap();
    assert!(matches!(
        volume.resume_read_stream(Path::new("f.bin"), &version, 70_000).await,
        Err(VolumeError::SourceChanged(_))
    ));
}

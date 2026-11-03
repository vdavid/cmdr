//! A copy's xattrs against real servers: `smb-consumer-fruit` (named streams and
//! `vfs_fruit`, what a Mac-friendly NAS runs) and the plain guest fixture, which
//! has no streams. What it pins and why: `DETAILS.md` § "Xattrs on copies".
//! Docker-gated, like every `smb_integration_*` test.

use super::test_support::*;
use super::*;
use cmdr_fs::metadata::{CarriedMetadata, MetadataAsk, MetadataClasses, Xattr};
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{StreamLength, Volume, VolumeReadStream, WriteMode, XattrReadMode, conformance};
use std::time::SystemTime;

const TAGS: &str = "com.apple.metadata:_kMDItemUserTags";

fn fruit_params() -> SmbConnectionParams {
    SmbConnectionParams::new("127.0.0.1", "public", smb2::testing::fruit_port(), None, None)
}

async fn fruit_volume() -> SmbVolume {
    let params = fruit_params();
    let volume_id = cmdr_fs::volume::smb_volume_id("127.0.0.1", params.port, "public");
    connect_smb_volume(
        "public",
        MountAnchor::at_share_root("/Volumes/fruit-test"),
        &volume_id,
        params,
        VolumeHost::detached(),
    )
    .await
    .unwrap_or_else(|e| panic!("Couldn't connect to smb-consumer-fruit. Is it running? ({e:?})"))
}

/// [`fruit_volume`] with no watcher, for counting frames (`make_counting_volume`
/// says why).
async fn fruit_counting_volume() -> SmbVolume {
    let params = fruit_params();
    let volume_id = cmdr_fs::volume::smb_volume_id("127.0.0.1", params.port, "public");
    let (client, tree) = build_session(&params, None)
        .await
        .unwrap_or_else(|e| panic!("Couldn't connect to smb-consumer-fruit. Is it running? ({e:?})"));
    SmbVolume::new(
        "public",
        MountAnchor::at_share_root("/Volumes/fruit-test"),
        &volume_id,
        params,
        client,
        tree,
        VolumeHost::detached(),
    )
}

/// A source of `size` bytes carrying a Finder tag.
struct TaggedSource {
    bytes: Option<Vec<u8>>,
    size: u64,
}

impl TaggedSource {
    fn new(size: usize) -> Self {
        Self {
            bytes: Some(vec![b't'; size]),
            size: size as u64,
        }
    }
}

impl VolumeReadStream for TaggedSource {
    fn next_chunk(
        &mut self,
    ) -> std::pin::Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move { self.bytes.take().map(Ok) })
    }
    fn total_size(&self) -> StreamLength {
        StreamLength::Known(self.size)
    }
    fn bytes_read(&self) -> u64 {
        if self.bytes.is_some() { 0 } else { self.size }
    }
    fn modified_at(&self) -> Option<SystemTime> {
        None
    }
    fn metadata(&self, wanted: MetadataClasses) -> CarriedMetadata {
        CarriedMetadata {
            xattrs: vec![Xattr {
                name: TAGS.into(),
                value: b"a tag plist".to_vec(),
            }],
            ..CarriedMetadata::default()
        }
        .restricted_to(wanted)
    }
}

/// A small file's known length takes the one-frame write; a big one goes
/// unannounced, which the streaming writer takes whatever the link measured.
fn length_for(size: usize) -> StreamLength {
    match size {
        0..=4096 => StreamLength::Known(size as u64),
        _ => StreamLength::Unknown,
    }
}

async fn tags_of(volume: &SmbVolume, path: &str) -> Option<Vec<u8>> {
    volume
        .read_xattr_many(&[PathBuf::from(path)], TAGS, XattrReadMode::Verified)
        .await
        .pop()
        .expect("one answer")
        .expect("the file reads")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_copy_keeps_xattrs_per_the_shared_contract() {
    let volume = fruit_volume().await;
    assert_eq!(volume.metadata_caps(), SmbVolume::NAMED_STREAM_CAPS);
    let base = test_dir_name();
    ensure_clean(&volume, &base).await;
    conformance::assert_a_copy_keeps_xattrs(&volume, Path::new(&base)).await;
    ensure_clean(&volume, &base).await;
}

/// The staged temp gets the streams, and the landing rename carries them to the
/// file's real name: the one-frame write and the streamed one alike.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_the_landing_rename_keeps_a_copys_xattrs() {
    let volume = fruit_volume().await;
    let base = test_dir_name();
    ensure_clean(&volume, &base).await;
    volume.create_directory(Path::new(&base)).await.unwrap();
    for (name, size) in [("small.txt", 10), ("large.bin", 2 * 1024 * 1024)] {
        let temp = format!("{base}/{name}.cmdr-tmp-xattrs");
        let landed = format!("{base}/{name}");
        let written = volume
            .write_from_stream(
                Path::new(&temp),
                WriteMode::CreateNew,
                length_for(size),
                Box::new(TaggedSource::new(size)),
                &|_| std::ops::ControlFlow::Continue(()),
            )
            .await
            .unwrap();
        assert!(written.refused().is_empty(), "{name}: {:?}", written.refused());
        volume
            .rename(Path::new(&temp), Path::new(&landed), false)
            .await
            .unwrap();
        assert_eq!(
            tags_of(&volume, &landed).await.as_deref(),
            Some(&b"a tag plist"[..]),
            "{name}"
        );
    }
    ensure_clean(&volume, &base).await;
}

/// A plain file read for a copy that keeps xattrs costs what a hinted read
/// always cost: ONE frame, now with the stream listing inside it (`(1, 4)`:
/// CREATE+READ+QUERY_INFO+CLOSE). A tagged file costs one frame more, its
/// values. A copy that never asks lists nothing (`(1, 3)`, as before).
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_plain_file_read_for_a_copy_costs_no_extra_frame() {
    let vol = fruit_counting_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();
    let plain = format!("{dir}/plain.txt");
    let tagged = format!("{dir}/tagged.txt");
    vol.create_file(Path::new(&plain), b"plain").await.unwrap();
    vol.create_file(Path::new(&tagged), b"tagged").await.unwrap();
    vol.write_xattr(Path::new(&tagged), TAGS, b"tags").await.unwrap();
    let keeps = MetadataAsk {
        values: MetadataClasses::XATTRS,
        outline: MetadataClasses::NONE,
    };

    let cost = async |path: &str, size: u64, ask: MetadataAsk| {
        let (requests, compounds) = request_counts(&vol).await;
        let stream = vol
            .open_read_stream_for_copy(Path::new(path), Some(size), ask)
            .await
            .unwrap();
        let xattrs = stream.metadata(MetadataClasses::XATTRS).xattrs;
        drop(drain(stream).await);
        let (requests_after, compounds_after) = request_counts(&vol).await;
        ((compounds_after - compounds, requests_after - requests), xattrs.len())
    };

    assert_eq!(cost(&plain, 5, keeps).await, ((1, 4), 0), "a plain file");
    assert_eq!(
        cost(&tagged, 6, keeps).await,
        ((2, 7), 1),
        "a tagged file: one frame more"
    );
    assert_eq!(
        cost(&tagged, 6, MetadataAsk::default()).await,
        ((1, 3), 0),
        "a read that asks nothing lists nothing"
    );

    ensure_clean(&vol, &dir).await;
}

/// A share with no streams claims no xattrs, so the engine counts a tagged
/// file's tags as left behind, and the copy lands; its reads list nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_share_without_streams_claims_no_xattrs_and_still_copies() {
    let volume = make_docker_volume().await;
    assert_eq!(volume.metadata_caps(), SmbVolume::METADATA_CAPS);
    let base = test_dir_name();
    ensure_clean(&volume, &base).await;
    volume.create_directory(Path::new(&base)).await.unwrap();
    let path = format!("{base}/tagged.txt");
    let written = volume
        .write_from_stream(
            Path::new(&path),
            WriteMode::CreateNew,
            StreamLength::Known(10),
            Box::new(TaggedSource::new(10)),
            &|_| std::ops::ControlFlow::Continue(()),
        )
        .await
        .unwrap();
    assert_eq!(written.bytes, 10);
    let ask = MetadataAsk::for_copy(volume.metadata_caps(), cmdr_fs::metadata::MetadataCaps::ALL);
    let stream = volume
        .open_read_stream_for_copy(Path::new(&path), Some(10), ask)
        .await
        .unwrap();
    assert!(stream.metadata(MetadataClasses::ALL).xattrs.is_empty());
    ensure_clean(&volume, &base).await;
}

/// A share that turns streams down mid-copy (its probe said yes) reports the
/// tags refused, a counted loss, and the copy lands whole: ❌ never a failed
/// write. Both write paths.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_refused_xattr_is_a_reported_loss_and_never_a_failed_copy() {
    let volume = make_docker_volume().await;
    // The guest share has no streams; pretend its probe said it had.
    volume.inner.stores_xattrs.store(true, Ordering::Relaxed);
    let base = test_dir_name();
    ensure_clean(&volume, &base).await;
    volume.create_directory(Path::new(&base)).await.unwrap();
    for (name, size) in [("small.txt", 10), ("large.bin", 2 * 1024 * 1024)] {
        let path = format!("{base}/{name}");
        let written = volume
            .write_from_stream(
                Path::new(&path),
                WriteMode::CreateNew,
                length_for(size),
                Box::new(TaggedSource::new(size)),
                &|_| std::ops::ControlFlow::Continue(()),
            )
            .await
            .unwrap_or_else(|e| panic!("{name}: a refused xattr must never fail the copy, got {e:?}"));
        assert_eq!(written.bytes, size as u64, "{name}");
        assert_eq!(written.refused(), MetadataClasses::XATTRS, "{name}");
        assert_eq!(
            volume.get_metadata(Path::new(&path)).await.unwrap().size,
            Some(size as u64),
            "{name}"
        );
    }
    ensure_clean(&volume, &base).await;
}

/// Copychunk moves a file's own bytes and never its named streams, so a copy
/// inside the share carries the tags across itself; a plain file's copy writes
/// none.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_server_side_copy_carries_the_streams_copychunk_leaves_behind() {
    let volume = fruit_volume().await;
    let base = test_dir_name();
    ensure_clean(&volume, &base).await;
    volume.create_directory(Path::new(&base)).await.unwrap();
    let tagged = format!("{base}/tagged.txt");
    let plain = format!("{base}/plain.txt");
    volume.create_file(Path::new(&tagged), b"tagged bytes").await.unwrap();
    volume.create_file(Path::new(&plain), b"plain bytes").await.unwrap();
    volume.write_xattr(Path::new(&tagged), TAGS, b"tags").await.unwrap();

    for (from, to) in [
        (&tagged, format!("{base}/tagged copy.txt")),
        (&plain, format!("{base}/plain copy.txt")),
    ] {
        let copied = volume
            .copy_within(Path::new(from), Path::new(&to), &|_, _| {
                std::ops::ControlFlow::Continue(())
            })
            .await
            .unwrap_or_else(|e| panic!("the fixture copies on the server: {e:?}"));
        assert_eq!(
            copied,
            volume.get_metadata(Path::new(from)).await.unwrap().size.unwrap()
        );
    }
    assert_eq!(
        tags_of(&volume, &format!("{base}/tagged copy.txt")).await.as_deref(),
        Some(&b"tags"[..])
    );
    assert_eq!(tags_of(&volume, &format!("{base}/plain copy.txt")).await, None);
    ensure_clean(&volume, &base).await;
}

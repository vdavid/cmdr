//! Extended attributes against real servers: `smb-consumer-fruit` (named streams and
//! `vfs_fruit`, what a Mac-friendly NAS runs) and the plain guest fixture, which has no
//! streams at all. Docker-gated, like every `smb_integration_*` test.

use super::test_support::*;
use super::*;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{Volume, VolumeError, XattrReadMode, conformance};

const TAGS: &str = "com.apple.metadata:_kMDItemUserTags";

/// macOS's own plist for `["Red\n6"]`, and for no tags at all.
const RED: &[u8] = &[
    0x62, 0x70, 0x6C, 0x69, 0x73, 0x74, 0x30, 0x30, 0xA1, 0x01, 0x55, 0x52, 0x65, 0x64, 0x0A, 0x36, 0x08, 0x0A, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10,
];

async fn fruit_volume() -> SmbVolume {
    let port = smb2::testing::fruit_port();
    let params = SmbConnectionParams::new("127.0.0.1", "public", port, None, None);
    let volume_id = cmdr_fs::volume::smb_volume_id("127.0.0.1", port, "public");
    connect_smb_volume(
        "public",
        MountAnchor::at_share_root("/Volumes/fruit-test"),
        &volume_id,
        params,
        VolumeHost::detached(),
    )
    .await
    .unwrap_or_else(|e| panic!("Failed to connect to smb-consumer-fruit on 127.0.0.1:{port}. Is it running? ({e:?})"))
}

fn one(paths: &str) -> Vec<PathBuf> {
    vec![PathBuf::from(paths)]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_xattrs_keep_the_shared_contract() {
    let volume = fruit_volume().await;
    let base = test_dir_name();
    ensure_clean(&volume, &base).await;
    volume.create_directory(Path::new(&base)).await.unwrap();
    let file = format!("{base}/plain.txt");
    volume.create_file(Path::new(&file), b"x").await.unwrap();

    conformance::assert_xattrs_keep_their_promises(&volume, Path::new(&file), Path::new(&format!("{base}/gone.txt")))
        .await;

    ensure_clean(&volume, &base).await;
}

/// A tag travels by its macOS name, on a file and on a folder, and survives a batch where
/// neighbors have none: the shape of a pane's sweep.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_finder_tag_reads_back_through_a_batch() {
    let volume = fruit_volume().await;
    let base = test_dir_name();
    ensure_clean(&volume, &base).await;
    volume.create_directory(Path::new(&base)).await.unwrap();

    let folder = format!("{base}/folder");
    volume.create_directory(Path::new(&folder)).await.unwrap();
    let mut paths = vec![PathBuf::from(&folder)];
    for i in 0..40 {
        let file = format!("{base}/file-{i}.txt");
        volume.create_file(Path::new(&file), b"x").await.unwrap();
        paths.push(PathBuf::from(file));
    }
    volume.write_xattr(&paths[0], TAGS, RED).await.unwrap();
    volume.write_xattr(&paths[7], TAGS, RED).await.unwrap();

    for mode in [XattrReadMode::Verified, XattrReadMode::AssumePathsExist] {
        let answers = volume.read_xattr_many(&paths, TAGS, mode).await;
        assert_eq!(answers.len(), paths.len());
        for (index, answer) in answers.iter().enumerate() {
            let expected = (index == 0 || index == 7).then_some(RED);
            assert_eq!(answer.as_ref().unwrap().as_deref(), expected, "row {index} in {mode:?}");
        }
    }

    ensure_clean(&volume, &base).await;
}

/// The whole point: the share's macOS mount going away changes nothing here.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_tags_work_with_the_mount_gone() {
    let volume = fruit_volume().await;
    let base = test_dir_name();
    ensure_clean(&volume, &base).await;
    volume.create_directory(Path::new(&base)).await.unwrap();
    let file = format!("{base}/f.txt");
    volume.create_file(Path::new(&file), b"x").await.unwrap();

    volume.note_root_mount_gone();
    assert!(!volume.paths_are_os_visible(), "fixture precondition: no mount");

    volume.write_xattr(Path::new(&file), TAGS, RED).await.unwrap();
    let answer = volume
        .read_xattr_many(&one(&file), TAGS, XattrReadMode::Verified)
        .await
        .pop()
        .unwrap();
    assert_eq!(answer.unwrap().as_deref(), Some(RED));

    ensure_clean(&volume, &base).await;
}

/// A plain Samba has no streams. It says so at connect, reads as "no attribute", and
/// refuses a write with the typed answer rather than an I/O failure.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_server_without_streams_says_so() {
    let volume = make_docker_volume().await;
    let base = test_dir_name();
    ensure_clean(&volume, &base).await;
    volume.create_directory(Path::new(&base)).await.unwrap();
    let file = format!("{base}/f.txt");
    volume.create_file(Path::new(&file), b"x").await.unwrap();

    assert!(!volume.supports_xattrs(), "the plain fixture reports no named streams");
    assert!(!volume.capabilities().can_store_xattrs, "and the menu hears it");
    let answer = volume
        .read_xattr_many(&one(&file), TAGS, XattrReadMode::Verified)
        .await
        .pop()
        .unwrap();
    assert_eq!(answer.unwrap(), None);
    assert!(matches!(
        volume.write_xattr(Path::new(&file), TAGS, RED).await,
        Err(VolumeError::NotSupported)
    ));

    ensure_clean(&volume, &base).await;
}

/// `fruit_volume` connects to a server that has them.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_server_with_streams_says_so() {
    assert!(fruit_volume().await.capabilities().can_store_xattrs);
}

/// Another client's tag arrives as a change the watch reports on the file, and the listing
/// host hears it should read that row's tags again, with no new size or date to go by.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_another_clients_tag_reaches_the_listing_host() {
    use cmdr_fs::testing::wait_until_async;
    use cmdr_fs::volume::host::listings::RecordingListings;
    use std::time::Duration;

    let recording = Arc::new(RecordingListings::new());
    let host = VolumeHost::builder().listings(recording.clone()).build();
    let port = smb2::testing::fruit_port();
    let params = SmbConnectionParams::new("127.0.0.1", "public", port, None, None);
    let volume_id = cmdr_fs::volume::smb_volume_id("127.0.0.1", port, "public");
    let mount = "/Volumes/fruit-watch-test";
    let watched = connect_smb_volume("public", MountAnchor::at_share_root(mount), &volume_id, params, host)
        .await
        .unwrap();
    let other = fruit_volume().await;

    let base = test_dir_name();
    ensure_clean(&other, &base).await;
    other.create_directory(Path::new(&base)).await.unwrap();
    let file = format!("{base}/f.txt");
    other.create_file(Path::new(&file), b"x").await.unwrap();

    let expected = Path::new(mount).join(&file);
    // Over and over, because the watch may arm after the first write and nothing says when.
    let other = Arc::new(other);
    let tagger = {
        let (other, file) = (other.clone(), file.clone());
        tokio::spawn(async move {
            loop {
                other.write_xattr(Path::new(&file), TAGS, RED).await.unwrap();
                // allowed-test-sleep: paces the repeated outside write, past the watcher's 200 ms debounce or no batch ever closes; the wait below is the probe
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        })
    };
    wait_until_async(Duration::from_secs(20), "a re-read of the row is asked for", || {
        recording.retagged().iter().any(|path| path == &expected)
    })
    .await;
    tagger.abort();

    drop(watched);
    ensure_clean(other.as_ref(), &base).await;
}

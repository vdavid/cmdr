use super::*;
use crate::testing::{FAKE_SERIAL, FakeAdbServer, FakeTree};

fn seeded() -> FakeTree {
    let mut tree = FakeTree::new();
    tree.add_file("/sdcard/hello.txt", b"hello, phone")
        .add_dir("/sdcard/DCIM")
        .add_symlink("/sdcard/link", "/sdcard/hello.txt");
    tree
}

async fn collect(session: &mut SyncSession, path: &str) -> Vec<SyncDirEntry> {
    let mut entries = Vec::new();
    session.list(path, &mut |e| entries.push(e)).await.unwrap();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

async fn stat_and_list(features: DeviceFeatures) {
    let server = FakeAdbServer::start(seeded()).await;
    let mut session = SyncSession::open(&server.endpoint(), FAKE_SERIAL, features)
        .await
        .unwrap();

    let file = session.stat("/sdcard/hello.txt").await.unwrap();
    assert!(file.exists());
    assert_eq!(file.kind(), SyncEntryKind::File);
    assert_eq!(file.size, 12);
    assert_eq!(file.mtime, crate::testing::DEFAULT_MTIME);

    let dir = session.stat("/sdcard/DCIM").await.unwrap();
    assert_eq!(dir.kind(), SyncEntryKind::Directory);

    let missing = session.stat("/sdcard/nope").await.unwrap();
    assert!(!missing.exists());
    if features.stat_v2 {
        assert_eq!(missing.errno, Some(crate::errors::ENOENT));
    } else {
        assert_eq!(missing.mode, 0);
    }

    let entries = collect(&mut session, "/sdcard").await;
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["DCIM", "hello.txt", "link"], "dot entries are skipped");
    assert_eq!(entries[2].stat.kind(), SyncEntryKind::Symlink);
    assert_eq!(entries[1].stat.size, 12);

    assert!(collect(&mut session, "/sdcard/DCIM").await.is_empty());
    session.quit().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn stat_and_list_v2() {
    stat_and_list(DeviceFeatures::all()).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn stat_and_list_v1() {
    stat_and_list(DeviceFeatures::default()).await;
}

async fn recv_and_send(features: DeviceFeatures) {
    let server = FakeAdbServer::start(seeded()).await;
    let mut session = SyncSession::open(&server.endpoint(), FAKE_SERIAL, features)
        .await
        .unwrap();

    session.recv_start("/sdcard/hello.txt").await.unwrap();
    let mut got = Vec::new();
    while let Some(chunk) = session.recv_chunk().await.unwrap() {
        got.extend_from_slice(&chunk);
    }
    assert_eq!(got, b"hello, phone");

    // Pushing a payload larger than one DATA packet splits it.
    let big: Vec<u8> = (0..(MAX_DATA_CHUNK * 3 + 17)).map(|i| (i % 251) as u8).collect();
    session.send_start("/sdcard/DCIM/big.bin", 0o644).await.unwrap();
    session.send_chunk(&big).await.unwrap();
    session.send_finish(1_700_000_000).await.unwrap();
    {
        let tree = server.tree();
        let tree = tree.lock().unwrap();
        assert_eq!(tree.file_bytes("/sdcard/DCIM/big.bin").unwrap(), big);
        assert_eq!(tree.get("/sdcard/DCIM/big.bin").unwrap().mtime(), 1_700_000_000);
    }

    // The session is reusable: pull what was pushed, in several chunks.
    session.recv_start("/sdcard/DCIM/big.bin").await.unwrap();
    let mut chunks = 0;
    let mut got = Vec::new();
    while let Some(chunk) = session.recv_chunk().await.unwrap() {
        assert!(chunk.len() <= MAX_DATA_CHUNK);
        chunks += 1;
        got.extend_from_slice(&chunk);
    }
    assert_eq!(got, big);
    assert_eq!(chunks, 4);
    session.quit().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn recv_and_send_v2() {
    recv_and_send(DeviceFeatures::all()).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn recv_and_send_v1() {
    recv_and_send(DeviceFeatures::default()).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fail_is_refused_with_the_message() {
    let server = FakeAdbServer::start(seeded()).await;
    let mut session = SyncSession::open(&server.endpoint(), FAKE_SERIAL, DeviceFeatures::all())
        .await
        .unwrap();
    session.recv_start("/sdcard/nope").await.unwrap();
    assert!(matches!(session.recv_chunk().await, Err(AdbError::Refused(msg)) if msg.contains("/sdcard/nope")));

    // Pushing into a missing directory fails at DONE.
    session.send_start("/sdcard/missing-dir/x", 0o644).await.unwrap();
    session.send_chunk(b"x").await.unwrap();
    assert!(matches!(session.send_finish(0).await, Err(AdbError::Refused(_))));
}

#[tokio::test(flavor = "multi_thread")]
async fn opening_on_an_unauthorized_device_is_refused() {
    let server = FakeAdbServer::start(seeded()).await;
    let mut dev = crate::testing::fake_device();
    dev.state = crate::devices::AdbDeviceState::Unauthorized;
    server.push_devices(vec![dev]);
    let err = SyncSession::open(&server.endpoint(), FAKE_SERIAL, DeviceFeatures::all())
        .await
        .unwrap_err();
    assert!(matches!(err, AdbError::Refused(_)));
}

#[test]
fn kind_reads_the_type_bits() {
    let stat = |mode| SyncStat {
        mode,
        size: 0,
        mtime: 0,
        errno: None,
    };
    assert_eq!(stat(0o100644).kind(), SyncEntryKind::File);
    assert_eq!(stat(0o040755).kind(), SyncEntryKind::Directory);
    assert_eq!(stat(0o120777).kind(), SyncEntryKind::Symlink);
    assert_eq!(stat(0o140000).kind(), SyncEntryKind::Other);
    assert!(!stat(0).exists());
    assert!(
        !SyncStat {
            errno: Some(2),
            ..stat(0o100644)
        }
        .exists()
    );
}

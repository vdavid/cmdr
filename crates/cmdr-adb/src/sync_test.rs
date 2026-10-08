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

/// A `STA2`/`DNT2` stat body with everything zeroed but the mode.
fn stat_v2_body(mode: u32) -> Vec<u8> {
    let mut body = vec![0u8; 68];
    body[20..24].copy_from_slice(&mode.to_le_bytes());
    body
}

// Regression: a length word straight off the wire sized the buffer, so one
// hostile `DNT2` made us allocate 4 GiB (found by the `adb_sync` fuzz target).
#[tokio::test]
async fn a_name_longer_than_the_protocol_allows_is_refused_before_allocating() {
    let mut device = b"DNT2".to_vec();
    device.extend(stat_v2_body(0o100644));
    device.extend(u32::MAX.to_le_bytes());
    let mut session = SyncSession::from_connection(AdbConnection::scripted(&device), DeviceFeatures::all());

    let err = session.list("/sdcard", &mut |_| {}).await.unwrap_err();

    assert!(matches!(err, AdbError::Protocol(_)), "got {err:?}");
}

#[tokio::test]
async fn a_data_chunk_longer_than_the_protocol_allows_is_refused() {
    let mut device = b"DATA".to_vec();
    device.extend(u32::try_from(MAX_DATA_CHUNK + 1).unwrap().to_le_bytes());
    let mut session = SyncSession::from_connection(AdbConnection::scripted(&device), DeviceFeatures::all());

    let err = session.recv_chunk().await.unwrap_err();

    assert!(matches!(err, AdbError::Protocol(_)), "got {err:?}");
}

#[tokio::test]
async fn a_full_size_data_chunk_still_reads() {
    let mut device = b"DATA".to_vec();
    device.extend(u32::try_from(MAX_DATA_CHUNK).unwrap().to_le_bytes());
    device.extend(vec![7u8; MAX_DATA_CHUNK]);
    let mut session = SyncSession::from_connection(AdbConnection::scripted(&device), DeviceFeatures::all());

    let chunk = session.recv_chunk().await.unwrap().unwrap();

    assert_eq!(chunk.len(), MAX_DATA_CHUNK);
}

#[test]
fn a_stat_reports_its_mtime_as_a_date_and_a_pre_epoch_one_as_none() {
    let stat = |mtime| SyncStat {
        mode: 0o100644,
        size: 0,
        mtime,
        errno: None,
    };
    let at = |secs| UNIX_EPOCH + Duration::from_secs(secs);
    assert_eq!(stat(1_611_909_015).modified_at(), Some(at(1_611_909_015)));
    // `STA2`/`DNT2` carry 64 bits: a date past 2106 stays whole.
    assert_eq!(stat(5_000_000_000).modified_at(), Some(at(5_000_000_000)));
    // The listing shows no date for these, so the stream mustn't invent one.
    assert_eq!(stat(-1).modified_at(), None);
}

#[test]
fn the_done_word_carries_whole_seconds_clamped_to_u32() {
    use std::time::{Duration, UNIX_EPOCH};
    assert_eq!(
        done_mtime_word(UNIX_EPOCH + Duration::from_millis(1_611_909_015_750)),
        1_611_909_015
    );
    assert_eq!(done_mtime_word(UNIX_EPOCH - Duration::from_secs(10)), 0);
    assert_eq!(
        done_mtime_word(UNIX_EPOCH + Duration::from_secs(5_000_000_000)),
        u32::MAX
    );
}

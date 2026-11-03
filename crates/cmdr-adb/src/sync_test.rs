use super::*;
use crate::lz4::Lz4Writer;
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
    assert_eq!(
        file.size_is_exact, features.stat_v2,
        "❗ a v1 size is the low 32 bits of an lstat, never an exact size"
    );
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
    assert_eq!(names, vec!["DCIM", "hello.txt", "link"]);
    assert_eq!(entries[2].stat.kind(), SyncEntryKind::Symlink);
    assert_eq!(entries[1].stat.size, 12);
    assert_eq!(entries[1].stat.size_is_exact, features.ls_v2);

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
        size_is_exact: true,
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

// Regression: `push_entry` dropped only `.` and `..`, so a device could list an
// empty name or one holding a `/`, which the volume would join into a path naming
// a different file (found by the `adb_sync` fuzz target's listing property).
#[tokio::test]
async fn a_listed_name_that_isnt_one_path_segment_is_dropped() {
    let mut device = Vec::new();
    for name in [&b""[..], b".", b"..", b"a/b", b"../up", b"kept"] {
        device.extend(b"DNT2");
        device.extend(stat_v2_body(0o100644));
        device.extend(u32::try_from(name.len()).unwrap().to_le_bytes());
        device.extend(name);
    }
    device.extend(b"DONE");
    device.extend([0u8; 72]);
    let mut session = SyncSession::from_connection(AdbConnection::scripted(&device), DeviceFeatures::all());

    let names: Vec<String> = collect(&mut session, "/sdcard")
        .await
        .into_iter()
        .map(|e| e.name)
        .collect();

    assert_eq!(names, ["kept"]);
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
        size_is_exact: true,
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

// The features string decides, read the way a connect reads it: `all()` here would
// prove nothing about what a real device's list turns on.
async fn session_for(server: &FakeAdbServer) -> SyncSession {
    let features = DeviceFeatures::fetch(&server.endpoint(), FAKE_SERIAL).await.unwrap();
    SyncSession::open(&server.endpoint(), FAKE_SERIAL, features)
        .await
        .unwrap()
}

async fn pull(session: &mut SyncSession, path: &str) -> Vec<u8> {
    session.recv_start(path).await.unwrap();
    let mut got = Vec::new();
    while let Some(chunk) = session.recv_chunk().await.unwrap() {
        assert!(chunk.len() <= MAX_DATA_CHUNK, "a chunk of {} bytes", chunk.len());
        got.extend_from_slice(&chunk);
    }
    got
}

async fn push(session: &mut SyncSession, path: &str, bytes: &[u8]) {
    session.send_start(path, 0o644).await.unwrap();
    session.send_chunk(bytes).await.unwrap();
    session.send_finish(1_700_000_000).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_that_advertises_lz4_gets_it_both_ways() {
    let server = FakeAdbServer::start(seeded()).await;
    let mut session = session_for(&server).await;

    assert_eq!(pull(&mut session, "/sdcard/hello.txt").await, b"hello, phone");
    let bytes: Vec<u8> = (0..MAX_DATA_CHUNK * 5).map(|i| (i / 300) as u8).collect();
    push(&mut session, "/sdcard/DCIM/big.bin", &bytes).await;
    assert_eq!(pull(&mut session, "/sdcard/DCIM/big.bin").await, bytes);
    session.quit().await;

    assert_eq!(server.transfer_flags(), [("RCV2", 2), ("SND2", 2), ("RCV2", 2)]);
    let tree = server.tree();
    assert_eq!(tree.lock().unwrap().file_bytes("/sdcard/DCIM/big.bin").unwrap(), bytes);
}

// The fake refuses the flag from a device without the feature, as an older `adbd`
// answers `unknown flags`, so a client that sent it anyway would fail here.
#[tokio::test(flavor = "multi_thread")]
async fn a_device_without_the_lz4_feature_gets_plain_v2_transfers() {
    let server = FakeAdbServer::start(seeded()).await;
    server.set_features("shell_v2,stat_v2,ls_v2,sendrecv_v2,sendrecv_v2_brotli,sendrecv_v2_zstd");
    let mut session = session_for(&server).await;

    assert_eq!(pull(&mut session, "/sdcard/hello.txt").await, b"hello, phone");
    push(&mut session, "/sdcard/DCIM/x.bin", b"plain").await;
    session.quit().await;

    assert_eq!(server.transfer_flags(), [("RCV2", 0), ("SND2", 0)]);
    let tree = server.tree();
    assert_eq!(tree.lock().unwrap().file_bytes("/sdcard/DCIM/x.bin").unwrap(), b"plain");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_without_sendrecv_v2_uses_the_v1_verbs_whatever_else_it_lists() {
    let server = FakeAdbServer::start(seeded()).await;
    server.set_features("shell_v2,sendrecv_v2_lz4");
    let mut session = session_for(&server).await;

    assert_eq!(pull(&mut session, "/sdcard/hello.txt").await, b"hello, phone");
    push(&mut session, "/sdcard/DCIM/x.bin", b"plain").await;
    session.quit().await;

    assert!(server.transfer_flags().is_empty(), "no flags word on the v1 verbs");
}

// ❗ adbd hangs on a `DONE` with no end mark before it, so zero bytes still owe a
// whole frame. The fake answers `decompress failed` instead of spinning.
#[tokio::test(flavor = "multi_thread")]
async fn an_empty_file_travels_compressed_both_ways() {
    let server = FakeAdbServer::start(seeded()).await;
    let mut session = session_for(&server).await;

    push(&mut session, "/sdcard/DCIM/empty.bin", b"").await;
    assert_eq!(pull(&mut session, "/sdcard/DCIM/empty.bin").await, b"");
    session.quit().await;

    assert_eq!(server.transfer_flags(), [("SND2", 2), ("RCV2", 2)]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_small_push_held_back_by_the_compressor_arrives_by_the_finish() {
    let server = FakeAdbServer::start(seeded()).await;
    let mut session = session_for(&server).await;

    session.send_start("/sdcard/DCIM/x.bin", 0o644).await.unwrap();
    for piece in [&b"one "[..], b"two ", b"three"] {
        session.send_chunk(piece).await.unwrap();
    }
    session.send_finish(1_700_000_000).await.unwrap();
    session.quit().await;

    let tree = server.tree();
    assert_eq!(
        tree.lock().unwrap().file_bytes("/sdcard/DCIM/x.bin").unwrap(),
        b"one two three"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_file_still_answers_fail_under_the_flag() {
    let server = FakeAdbServer::start(seeded()).await;
    let mut session = session_for(&server).await;
    session.recv_start("/sdcard/nope").await.unwrap();
    assert!(matches!(session.recv_chunk().await, Err(AdbError::Refused(_))));
}

/// What a device says to `RCV2` under the flag: `frame` cut into `DATA` packets,
/// then `DONE`.
fn compressed_answer(frame: &[u8]) -> Vec<u8> {
    let mut device = Vec::new();
    for packet in frame.chunks(MAX_DATA_CHUNK) {
        device.extend(b"DATA");
        device.extend(u32::try_from(packet.len()).unwrap().to_le_bytes());
        device.extend(packet);
    }
    device.extend(b"DONE");
    device.extend(0u32.to_le_bytes());
    device
}

fn frame_of(data: &[u8]) -> Vec<u8> {
    let mut writer = Lz4Writer::new();
    let mut wire = writer.write(data).unwrap();
    wire.extend(writer.finish().unwrap());
    wire
}

// The compression bomb: 64 MiB of zeros in a few hundred KiB on the wire. Whatever
// it expands to has to come out one capped chunk per call.
#[tokio::test]
async fn a_compression_bomb_comes_out_in_capped_chunks() {
    const BOMB: usize = 64 * 1024 * 1024;
    let frame = frame_of(&vec![0u8; BOMB]);
    assert!(frame.len() < BOMB / 100, "not much of a bomb: {} bytes", frame.len());
    let device = compressed_answer(&frame);
    let mut session = SyncSession::from_connection(AdbConnection::scripted(&device), DeviceFeatures::all());

    session.recv_start("/sdcard/bomb").await.unwrap();
    let (mut total, mut chunks) = (0, 0);
    while let Some(chunk) = session.recv_chunk().await.unwrap() {
        assert!(chunk.len() <= MAX_DATA_CHUNK, "a chunk of {} bytes", chunk.len());
        total += chunk.len();
        chunks += 1;
    }

    assert_eq!(total, BOMB);
    assert!(chunks >= BOMB / MAX_DATA_CHUNK);
}

// A device that stops early must not read as a file that was shorter: on a stream
// of unknown length nothing downstream would notice.
#[tokio::test]
async fn a_done_before_the_frames_end_mark_is_refused() {
    let frame = frame_of(&vec![7u8; MAX_DATA_CHUNK * 2]);
    let device = compressed_answer(&frame[..frame.len() - 4]);
    let mut session = SyncSession::from_connection(AdbConnection::scripted(&device), DeviceFeatures::all());

    session.recv_start("/sdcard/a").await.unwrap();
    let outcome = loop {
        match session.recv_chunk().await {
            Ok(Some(_)) => {}
            other => break other,
        }
    };

    assert!(matches!(outcome, Err(AdbError::Protocol(_))), "got {outcome:?}");
}

#[tokio::test]
async fn a_compressed_pull_still_refuses_an_oversized_packet() {
    let mut device = b"DATA".to_vec();
    device.extend(u32::MAX.to_le_bytes());
    let mut session = SyncSession::from_connection(AdbConnection::scripted(&device), DeviceFeatures::all());

    session.recv_start("/sdcard/a").await.unwrap();

    assert!(matches!(session.recv_chunk().await, Err(AdbError::Protocol(_))));
}

// A session reuses its socket, and the second pull's frame must start clean.
#[tokio::test]
async fn a_second_compressed_pull_on_a_session_starts_a_new_frame() {
    let mut device = compressed_answer(&frame_of(b"first"));
    device.extend(compressed_answer(&frame_of(b"second")));
    let mut session = SyncSession::from_connection(AdbConnection::scripted(&device), DeviceFeatures::all());

    assert_eq!(pull(&mut session, "/sdcard/a").await, b"first");
    assert_eq!(pull(&mut session, "/sdcard/b").await, b"second");
}

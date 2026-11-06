//! A held reader and a streaming download carrying on after their connection
//! drops, against the Docker share (`resume.rs`).
//!
//! The drop is smb2's own verdict on a dead link (`Connection::mark_dead`, what
//! its liveness checks reach on a real outage, here at once), with the reader's
//! cell also cutting the network first through a [`BlackHoleProxy`], so the
//! server still holds the old handle while the new session reopens the file.
//! A file rewritten while the handle was away is refused, ❌ never spliced.
//!
//! `#[ignore]`d like every `smb_integration_` test; start the containers with
//! `apps/desktop/test/smb-servers/start.sh`.

use super::black_hole_proxy::BlackHoleProxy;
use super::test_support::*;
use super::*;
use cmdr_fs::volume::StreamLength;

/// Bytes whose value depends on their offset, so a skipped or doubled span
/// can't compare equal.
fn patterned(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

async fn seeded(vol: &SmbVolume, len: usize) -> (String, Vec<u8>) {
    let dir = test_dir_name();
    ensure_clean(vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();
    let path = format!("{dir}/held.bin");
    let data = patterned(len);
    vol.create_file(Path::new(&path), &data).await.unwrap();
    (path, data)
}

/// Overwrites `path` in place from a session of its own (straight to the
/// container, past any proxy), the way another client would while a handle was
/// away: same file, new bytes, new length.
async fn rewrite(path: &str, bytes: &[u8]) {
    let other = make_docker_volume().await;
    let SessionLease { tree, mut conn, .. } = other.clone_session().await.expect("a live share leases");
    // A dropped session's open holds the file until the server notices the
    // closed socket, a moment after the drop.
    for _ in 0..50 {
        match tree.write_file(&mut conn, path, bytes).await {
            Ok(_) => return,
            Err(e) if e.kind() == smb2::ErrorKind::SharingViolation => {
                // allowed-test-sleep: the server lets go of a dropped session's open on its own schedule, and only a write can tell
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(e) => panic!("rewriting {path}: {e}"),
        }
    }
    panic!("{path} stayed open elsewhere for 5 s");
}

/// Kills the main session's connection the way smb2 does once it proves the
/// link dead.
async fn drop_the_main_connection(vol: &SmbVolume) {
    vol.clone_session().await.expect("a live share leases").conn.mark_dead();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_held_reader_carries_on_after_its_connection_drops() {
    let proxy = BlackHoleProxy::start(guest_port()).await;
    let vol = make_docker_volume_through(proxy.port(), VolumeHost::detached()).await;
    let (path, data) = seeded(&vol, 1024 * 1024).await;
    let reader = vol.open_random_reader(Path::new(&path)).await.unwrap();
    assert_eq!(reader.read_at(0, 4096).await.unwrap(), data[..4096]);

    // The network goes quiet with no FIN, so the server keeps the old open.
    proxy.black_hole();
    drop_the_main_connection(&vol).await;

    let read = reader.read_at(512 * 1024, 64 * 1024).await;
    assert_eq!(
        read.expect("the reader reopens on the new session and reads on"),
        data[512 * 1024..576 * 1024]
    );
    assert_eq!(vol.session_state(), ConnectionState::Direct);
    reader.close().await.unwrap();
    ensure_clean(&vol, path.split('/').next().unwrap()).await;
}

/// Reads racing the same dead handle all carry on, on the one reader the first
/// of them reopened.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_concurrent_reads_on_a_dead_handle_all_carry_on() {
    let vol = make_docker_volume().await;
    let (path, data) = seeded(&vol, 1024 * 1024).await;
    let reader = vol.open_random_reader(Path::new(&path)).await.unwrap();
    drop_the_main_connection(&vol).await;

    let reads = (0..8u64).map(|i| reader.read_at(i * 64 * 1024, 64 * 1024));
    for (i, read) in futures_util::future::join_all(reads).await.into_iter().enumerate() {
        let from = i * 64 * 1024;
        assert_eq!(read.unwrap(), data[from..from + 64 * 1024], "read {i}");
    }
    reader.close().await.unwrap();
    ensure_clean(&vol, path.split('/').next().unwrap()).await;
}

/// A file rewritten while the reader was away is the source changing under it:
/// `SourceChanged`, which the viewer reloads on, on this read and every one
/// after it. Bytes of the new version are never served as the old one's.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_held_reader_refuses_a_file_rewritten_while_it_was_away() {
    let vol = make_docker_volume().await;
    let (path, data) = seeded(&vol, 256 * 1024).await;
    let reader = vol.open_random_reader(Path::new(&path)).await.unwrap();
    assert_eq!(reader.read_at(0, 4096).await.unwrap(), data[..4096]);

    drop_the_main_connection(&vol).await;

    rewrite(&path, &patterned(300 * 1024)).await;

    for attempt in 0..2 {
        match reader.read_at(4096, 4096).await {
            Err(VolumeError::SourceChanged(_)) => {}
            other => panic!("read {attempt}: expected SourceChanged, got {other:?}"),
        }
    }
    reader.close().await.unwrap();
    ensure_clean(&vol, path.split('/').next().unwrap()).await;
}

/// A copy's download carries on from the byte it got to, on whichever session
/// is up after its own dropped, so the copy gets every byte exactly once.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_download_resumes_after_its_connection_drops_with_no_byte_lost_or_doubled() {
    let vol = make_docker_volume().await;
    let (path, data) = seeded(&vol, 32 * 1024 * 1024).await;
    let mut stream = vol.open_read_stream(Path::new(&path)).await.unwrap();
    let mut got = stream.next_chunk().await.unwrap().unwrap();

    // The download rides the transfer session; its connection goes.
    vol.transfer_lease()
        .await
        .expect("a live share leases")
        .conn
        .mark_dead();

    while let Some(chunk) = stream.next_chunk().await {
        got.extend(chunk.expect("the download resumes rather than fail"));
    }
    assert_eq!(got.len(), data.len());
    assert!(got == data, "the resumed download must equal the file byte for byte");
    ensure_clean(&vol, path.split('/').next().unwrap()).await;
}

/// A file rewritten while the download was away stops the copy with
/// `SourceChanged`, ❌ never a quiet restart or a spliced file. The network
/// stays down (the proxy refuses new connections) until the rewrite has
/// landed, so the resume can't beat it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_download_refuses_a_file_rewritten_while_it_was_away() {
    let proxy = BlackHoleProxy::start(guest_port()).await;
    let vol = make_docker_volume_through(proxy.port(), VolumeHost::detached()).await;
    let (path, _) = seeded(&vol, 32 * 1024 * 1024).await;
    let mut stream = vol.open_read_stream(Path::new(&path)).await.unwrap();
    stream.next_chunk().await.unwrap().unwrap();

    // Both of the share's connections go, and nothing new gets through yet.
    proxy.refuse_new(true);
    vol.transfer_lease()
        .await
        .expect("a live share leases")
        .conn
        .mark_dead();
    drop_the_main_connection(&vol).await;
    rewrite(&path, b"a different file").await;
    proxy.refuse_new(false);

    let mut ended_with = None;
    while let Some(chunk) = stream.next_chunk().await {
        if let Err(e) = chunk {
            ended_with = Some(e);
            break;
        }
    }
    assert!(
        matches!(ended_with, Some(VolumeError::SourceChanged(_))),
        "expected SourceChanged, got {ended_with:?}"
    );
    drop(stream);
    ensure_clean(&vol, path.split('/').next().unwrap()).await;
}

/// A copy that waited out a long outage carries a download on through
/// `Volume::resume_read_stream`: the version the first stream named, the bytes
/// from the offset on, and nothing before it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_download_resumes_from_an_offset_on_its_named_version() {
    let vol = make_docker_volume().await;
    let (path, data) = seeded(&vol, 4 * 1024 * 1024).await;
    let first = vol.open_read_stream(Path::new(&path)).await.unwrap();
    let version = first
        .source_version()
        .expect("a streamed SMB download names its version");
    drop(first);

    let offset = 1_500_000;
    let mut rest = vol
        .resume_read_stream(Path::new(&path), &version, offset)
        .await
        .expect("the same file resumes");
    assert_eq!(
        rest.total_size(),
        StreamLength::Known(data.len() as u64),
        "the whole file's size"
    );
    let mut got = Vec::new();
    while let Some(chunk) = rest.next_chunk().await {
        got.extend(chunk.unwrap());
    }
    assert!(got == data[offset as usize..], "exactly the bytes from the offset on");
    ensure_clean(&vol, path.split('/').next().unwrap()).await;
}

/// A file rewritten since the first stream opened it is refused before a byte
/// is read: `SourceChanged`, carrying the path.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_resume_refuses_a_file_rewritten_since() {
    let vol = make_docker_volume().await;
    let (path, _) = seeded(&vol, 4 * 1024 * 1024).await;
    let first = vol.open_read_stream(Path::new(&path)).await.unwrap();
    let version = first
        .source_version()
        .expect("a streamed SMB download names its version");
    drop(first);
    rewrite(&path, b"a different file").await;

    match vol.resume_read_stream(Path::new(&path), &version, 1_000).await {
        Err(VolumeError::SourceChanged(_)) => {}
        Err(other) => panic!("expected SourceChanged, got {other:?}"),
        Ok(_) => panic!("expected SourceChanged, got a stream"),
    }
    ensure_clean(&vol, path.split('/').next().unwrap()).await;
}

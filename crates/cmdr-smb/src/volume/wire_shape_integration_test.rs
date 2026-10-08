//! What a byte-path operation COSTS on the wire, against a real server
//! (requires Docker SMB containers).
//!
//! Sibling to the two byte-path suites and a different question from either:
//! they ask whether the bytes are right, this one asks how many frames carried
//! them and how many such operations the server's credit window can carry at
//! once. So the hinted read is here for its ONE compound frame (and where that
//! frame stops paying off, the connection's `quick_read_limit`), and the
//! streamed read for ending at its last byte rather than a round trip later,
//! while the hinted read's size-drift behavior stays in
//! `read_stream_integration_test.rs`, and the single-shot write promise is here (both the wire proof and the
//! `write_is_single_shot` predicate the transfer layer skips its `.cmdr-tmp-*`
//! staging on), while everything else `write_from_stream` does stays in
//! `write_stream_integration_test.rs`.
//!
//! The copy-concurrency cell asserts neither byte path: its subject is the
//! credit window, which the sized read and the slot clamp are the two halves of
//! (`DETAILS.md` § "Copy concurrency and the credit window").
//!
//! Every test here is `#[ignore]`d so default runs skip it. Start the containers
//! with `apps/desktop/test/smb-servers/start.sh`, then run
//! `cargo nextest run smb_integration --run-ignored all`. Declared as a
//! `#[cfg(test)]` submodule of `volume`; shared helpers come from
//! `super::test_support`.

use super::streams::InlineReadStream;
use super::test_support::*;
use super::*;
use cmdr_fs::volume::{StreamLength, WriteMode};

/// `(requests_sent, compound_requests_sent)` on the volume's main connection.
async fn request_counts(vol: &SmbVolume) -> (u64, u64) {
    let d = vol.diagnostics().await.expect("a connected volume has diagnostics");
    (
        d.primary.metrics.requests_sent,
        d.primary.metrics.compound_requests_sent,
    )
}

// ── The hinted read: one sized frame ───────────────────────────

/// A hinted read of a small file has to leave as ONE compound frame
/// (CREATE+READ+CLOSE): that single round trip is the whole reason the fast path
/// exists, and it's what a 100k-file copy multiplies.
///
/// The READ inside it is sized to the hint, which is invisible on the frame
/// count and very visible on the connection's credit budget: an unsized READ
/// books `max_read` (8 MB, 128 credits) whatever the file weighs, so ten
/// concurrent small reads ask for 1,300 credits against a ~512-credit window and
/// most of them park instead of copying.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_hinted_read_leaves_as_one_compound_frame() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let data: Vec<u8> = (0..=255u8).cycle().take(128 * 1024).collect();
    let path = format!("{}/hinted.bin", dir);
    vol.create_file(Path::new(&path), &data).await.unwrap();

    let (requests_before, compounds_before) = request_counts(&vol).await;
    let stream = vol
        .open_read_stream_with_hint(Path::new(&path), Some(data.len() as u64))
        .await
        .unwrap();
    let got = drain(stream).await;
    let (requests_after, compounds_after) = request_counts(&vol).await;

    assert_eq!(got, data, "the fast path must serve the file byte for byte");
    // ONE compound frame, three ops inside it (verified against Samba in the
    // `smb-consumer` container on smb2 0.21.0, 2026-09-02). The two counters
    // answer different questions: `compound_requests_sent` counts CHAINS, which
    // is the frame count, while `requests_sent` counts every sub-op of every
    // chain (`MetricsSnapshot`'s own docs say so). So `(1, 3)` is one frame
    // carrying CREATE+READ+CLOSE, and reading that 3 as a round-trip count is
    // the mistake this comment exists to head off.
    // Asserting the PAIR is what gives the cell its teeth: a 3-RTT streaming
    // open reads as `(0, 3)`, and a loose round trip alongside the compound as
    // `(1, 4)`. Same shape as the write cell below. The read's date rides on
    // its own CREATE response, so a stat for it would show here as `(2, 7)`.
    assert_eq!(
        (compounds_after - compounds_before, requests_after - requests_before),
        (1, 3),
        "a hinted small read must leave as ONE compound frame carrying CREATE+READ+CLOSE, its date included; a 3-RTT streaming open is what this prevents"
    );

    ensure_clean(&vol, &dir).await;
}

/// On a COLD connection (no download has measured the link yet), the compound
/// path ends at ONE download chunk (`smb2::DOWNLOAD_CHUNK_SIZE`), not at the
/// server's `max_read` (8 MiB on the fixture). A file one byte past it has to
/// stream: with nothing known about the link, the compound could carry it as a
/// single READ with no progress, queued ahead of every listing on the
/// connection until the whole body arrived (cmdr-reports#15: 23 s for 8 MiB on
/// a 375 KB/s link).
///
/// The streaming side reads `(0, 4)`: CREATE, two READs, CLOSE as loose
/// requests, and the body reaches the consumer in more than one chunk, which is
/// what lets the copy's progress and liveness watchdog see it move.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_the_compound_read_stops_at_one_download_chunk() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let chunk = smb2::DOWNLOAD_CHUNK_SIZE as usize;
    let (_tree, conn) = vol.clone_session().await.unwrap();
    assert_eq!(
        conn.quick_read_limit(),
        chunk as u64,
        "a fresh connection has measured nothing, so its limit is one chunk"
    );
    for (size, expected_frames, expected_chunks, what) in [
        (chunk, (1, 3), 1, "a file of exactly one chunk takes the compound path"),
        (chunk + 1, (0, 4), 2, "a file one byte over a chunk streams"),
    ] {
        let data: Vec<u8> = (0..=255u8).cycle().take(size).collect();
        let path = format!("{}/boundary-{size}.bin", dir);
        vol.create_file(Path::new(&path), &data).await.unwrap();

        let (requests_before, compounds_before) = request_counts(&vol).await;
        let mut stream = vol
            .open_read_stream_with_hint(Path::new(&path), Some(size as u64))
            .await
            .unwrap();
        let mut got = Vec::new();
        let mut chunks = 0;
        while let Some(chunk) = stream.next_chunk().await {
            got.extend_from_slice(&chunk.unwrap());
            chunks += 1;
        }
        let (requests_after, compounds_after) = request_counts(&vol).await;

        assert_eq!(got, data, "{what}: the bytes must arrive whole");
        assert_eq!(
            (compounds_after - compounds_before, requests_after - requests_before),
            expected_frames,
            "{what}: wrong frame shape"
        );
        assert_eq!(chunks, expected_chunks, "{what}: wrong chunk count");
    }

    ensure_clean(&vol, &dir).await;
}

/// Once a download has measured the link, the compound path takes whatever the
/// link moves in 250 ms (`Connection::quick_read_limit`), so a multi-chunk file
/// on a fast link leaves as ONE frame again. That's the round trip streaming
/// can't save: at +60 ms a 4 MiB file's last byte arrived in 139 ms compounded
/// against 203 ms streamed (smb2's `benchmarks/read-ahead/results/close-and-quick-read.md`,
/// smb2 0.24.2, 2026-09-23).
///
/// The rate lives on the connection, so every clone the next read takes sees
/// what the warm-up download measured.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_measured_fast_link_compounds_a_multi_chunk_file() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    // Any download of two or more chunks measures the link.
    let warm_up: Vec<u8> = (0..=255u8).cycle().take(8 * 1024 * 1024).collect();
    let warm_up_path = format!("{}/warm-up.bin", dir);
    vol.create_file(Path::new(&warm_up_path), &warm_up).await.unwrap();
    drain(vol.open_read_stream(Path::new(&warm_up_path)).await.unwrap()).await;

    let size = 2 * 1024 * 1024;
    let (_tree, conn) = vol.clone_session().await.unwrap();
    assert!(
        conn.quick_read_limit() >= size as u64,
        "loopback moves far more than 2 MiB in 250 ms, so the limit should have risen from one chunk, got {}",
        conn.quick_read_limit()
    );

    let data: Vec<u8> = (0..=255u8).cycle().take(size).collect();
    let path = format!("{}/two-mib.bin", dir);
    vol.create_file(Path::new(&path), &data).await.unwrap();

    let (requests_before, compounds_before) = request_counts(&vol).await;
    let stream = vol
        .open_read_stream_with_hint(Path::new(&path), Some(size as u64))
        .await
        .unwrap();
    let got = drain(stream).await;
    let (requests_after, compounds_after) = request_counts(&vol).await;

    assert_eq!(got, data, "the compound path must serve the file byte for byte");
    assert_eq!(
        (compounds_after - compounds_before, requests_after - requests_before),
        (1, 3),
        "a 2 MiB file under a measured fast link's limit must leave as ONE compound frame"
    );

    ensure_clean(&vol, &dir).await;
}

// ── The streamed read: ends at the last byte ───────────────────

/// Connects to the `slow` fixture (200 ms of netem delay on the server's side),
/// where a round trip is long enough to see on a clock.
async fn make_slow_docker_volume() -> SmbVolume {
    let port = smb2::testing::slow_port();
    let volume_id = cmdr_fs::volume::smb_volume_id("127.0.0.1", port, "public");
    connect_smb_volume(
        "public",
        MountAnchor::at_share_root(TEST_MOUNT_ROOT),
        &volume_id,
        SmbConnectionParams::new("127.0.0.1", "public", port, None, None),
        VolumeHost::detached(),
    )
    .await
    .unwrap_or_else(|e| {
        panic!("Failed to connect to the slow SMB container at 127.0.0.1:{port}. Is it running? ({e:?})")
    })
}

/// A streamed read ends the moment its last byte arrives, not a round trip
/// later when the CLOSE's answer does. smb2 puts the CLOSE on the wire before it
/// hands out the last chunk, so the handle is closing either way; a consumer
/// that waited for the answer paid 200 ms per file on this fixture for nothing.
///
/// Timed from the last chunk to end-of-stream, so the fixture's own delay can't
/// blur it: without the early end, that gap IS one round trip.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_streamed_read_ends_before_the_close_is_answered() {
    let vol = make_slow_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    // Two chunks, so it streams on this cold connection.
    let data: Vec<u8> = (0..=255u8).cycle().take(1024 * 1024).collect();
    let path = format!("{}/two-chunks.bin", dir);
    vol.create_file(Path::new(&path), &data).await.unwrap();

    let mut stream = vol.open_read_stream(Path::new(&path)).await.unwrap();
    let mut got = Vec::new();
    let mut last_chunk_at = None;
    while let Some(chunk) = stream.next_chunk().await {
        got.extend_from_slice(&chunk.unwrap());
        last_chunk_at = Some(std::time::Instant::now());
    }
    let tail = last_chunk_at.expect("a 1 MiB file has chunks").elapsed();

    assert_eq!(got, data, "the bytes must arrive whole");
    assert!(
        tail < Duration::from_millis(100),
        "the stream must end at the last byte, not wait out the CLOSE's 200 ms round trip; it ended {tail:?} after the last chunk"
    );

    ensure_clean(&vol, &dir).await;
}

// ── The single-shot write promise (staging exemption) ──────────

/// The transfer layer skips its `.cmdr-tmp-*` staging for a write this backend
/// promises to land in ONE shot, so the promise has to hold against a real
/// server: a write that fits one WRITE must leave as a single compound frame
/// (CREATE+WRITE+FLUSH+CLOSE), which is what makes it all-or-nothing.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_single_shot_write_leaves_as_one_compound_frame() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let data = vec![0xABu8; 4096];
    let size = data.len() as u64;
    assert!(
        vol.write_is_single_shot(StreamLength::Known(size)).await,
        "4 KiB fits one WRITE on every SMB2 dialect"
    );

    let smb_path = format!("{}/one-shot.bin", dir);
    let (requests_before, compounds_before) = request_counts(&vol).await;
    let written = vol
        .write_from_stream(
            Path::new(&smb_path),
            WriteMode::CreateOrReplace,
            StreamLength::Known(size),
            Box::new(InlineReadStream::new(data.clone(), None)),
            &|_| std::ops::ControlFlow::Continue(()),
        )
        .await
        .unwrap();
    let (requests_after, compounds_after) = request_counts(&vol).await;

    assert_eq!(written, size);
    // TWO compound frames leave the wire, four ops each (verified against Samba
    // in the `smb-consumer` container, 2026-08-01): the write's
    // CREATE+WRITE+FLUSH+CLOSE, then the CREATE+QUERY_INFO+CLOSE stat every SMB
    // write ends with to patch the listing cache. What matters is that NOTHING
    // outside a compound frame went out — a streaming write would show its
    // separate CREATE, WRITE, and CLOSE round trips here.
    assert_eq!(
        (compounds_after - compounds_before, requests_after - requests_before),
        (2, 8),
        "the write must leave as ONE compound frame (plus the post-write stat), with no loose round trips"
    );

    // The bytes are at the FINAL name the moment the write returns — no temp,
    // nothing to land.
    let mut stream = vol.open_read_stream(Path::new(&smb_path)).await.unwrap();
    let mut read_back = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        read_back.extend_from_slice(&chunk);
    }
    assert_eq!(read_back, data);
    let names: Vec<String> = vol
        .list_directory(Path::new(&dir), None)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, vec!["one-shot.bin".to_string()], "no leftovers; got {names:?}");

    ensure_clean(&vol, &dir).await;
}

/// A dated source costs a one-shot write ONE more compound frame: the write's
/// CLOSE stamps the server's own date, so the source's goes on by path after it
/// (CREATE+SET_INFO+CLOSE). Still nothing loose, and the stamp lands.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_dated_single_shot_write_adds_one_frame_for_its_date() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let data = vec![0xABu8; 4096];
    let size = data.len() as u64;
    let date_secs = cmdr_fs::volume::conformance::SOURCE_DATE_SECS;
    let date = std::time::UNIX_EPOCH + Duration::from_secs(date_secs);
    let smb_path = format!("{}/one-shot-dated.bin", dir);
    let (requests_before, compounds_before) = request_counts(&vol).await;
    vol.write_from_stream(
        Path::new(&smb_path),
        WriteMode::CreateOrReplace,
        StreamLength::Known(size),
        Box::new(InlineReadStream::new(data, Some(date))),
        &|_| std::ops::ControlFlow::Continue(()),
    )
    .await
    .unwrap();
    let (requests_after, compounds_after) = request_counts(&vol).await;

    // The write's four ops, the date's three, the post-write stat's four.
    assert_eq!(
        (compounds_after - compounds_before, requests_after - requests_before),
        (3, 11),
        "a dated one-shot write must add exactly one compound frame for its date, with no loose round trips"
    );
    let listed = vol.get_metadata(Path::new(&smb_path)).await.unwrap().modified_at;
    assert_eq!(
        listed,
        Some(date_secs),
        "the one-shot write must keep the source's date"
    );

    ensure_clean(&vol, &dir).await;
}

/// The other direction against a real server: a file bigger than the uplink
/// moves in about 250 ms gets NO promise, so the transfer layer stages it and it
/// streams. The promise reads smb2's `quick_write_limit`, which on a connection
/// that hasn't measured its uplink yet is one upload chunk (512 KiB), however
/// big the negotiated `max_write`: one frame that size queues everything else on
/// the connection behind it with no progress (a `stat` waited 23 s behind an
/// 8 MiB upload at 375 KB/s). ❌ The answer must come from smb2's limit, never
/// from `max_write` or a size the caller picked.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_cold_connection_promises_one_shot_up_to_one_upload_chunk() {
    let vol = make_docker_volume().await;
    let max_write = vol
        .negotiated_max_write()
        .await
        .expect("a connected volume has negotiated params");
    let chunk = u64::from(smb2::UPLOAD_CHUNK_SIZE);
    assert!(
        max_write > chunk,
        "the fixture must negotiate more than one chunk per WRITE, or this proves nothing"
    );

    assert!(
        vol.write_is_single_shot(StreamLength::Known(chunk)).await,
        "one chunk is one WRITE either way"
    );
    assert!(
        !vol.write_is_single_shot(StreamLength::Known(chunk + 1)).await,
        "a cold connection streams anything over one chunk, so the transfer stages it"
    );
    assert!(
        !vol.write_is_single_shot(StreamLength::Known(max_write)).await,
        "one WRITE's worth is no promise until the uplink has shown it moves that fast"
    );
    assert!(
        !vol.write_is_single_shot(StreamLength::Known(0)).await,
        "an empty file has no WRITE to compound with; it takes the streaming writer"
    );
}

/// A staged write over the connection's `quick_write_limit` streams through the
/// `FileWriter`, even though one frame could carry it: that's what keeps a slow
/// uplink from queueing the whole file ahead of every listing. The tell is the
/// upload rate the stream leaves on the connection (two 512 KiB WRITEs measure
/// the uplink; one compound frame measures nothing), which is also what lifts
/// the limit for the next file.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_staged_write_over_the_quick_write_limit_streams() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();
    let data: Vec<u8> = (0..=252u8).cycle().take(1024 * 1024).collect();
    let size = data.len() as u64;
    let (_tree, conn) = vol.clone_session().await.unwrap();
    assert!(
        size > conn.quick_write_limit() && size <= conn.compound_write_limit(),
        "1 MiB must fit one frame by credits and size, and still be over a cold connection's limit"
    );

    let temp = format!(
        "{}/one-mib.bin{}{}",
        dir,
        cmdr_fs::staging::STAGING_TEMP_MARKER,
        "quick-test"
    );
    let written = vol
        .write_from_stream(
            Path::new(&temp),
            WriteMode::CreateOrReplace,
            StreamLength::Known(size),
            Box::new(InlineReadStream::new(data.clone(), None)),
            &|_| std::ops::ControlFlow::Continue(()),
        )
        .await
        .unwrap();

    assert_eq!(written, size);
    assert!(
        conn.upload_rate_hint().is_some(),
        "the write must have streamed as two WRITEs, which leave the uplink's rate on the connection"
    );
    assert_eq!(drain(vol.open_read_stream(Path::new(&temp)).await.unwrap()).await, data);

    ensure_clean(&vol, &dir).await;
}

/// Once an upload has measured a fast uplink, the promise covers what it moves
/// in 250 ms (up to `compound_write_limit`), and a promised write to the user's
/// real name still leaves as ONE compound frame: the promise and the write read
/// the same limit, and the real name's frame limit covers every size a promise
/// could have been made under.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_warm_uplink_lifts_the_promise_and_the_promised_write_is_one_frame() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    // Warm the uplink with a staged 8 MiB stream (loopback moves it at hundreds
    // of MB/s, so 250 ms of it clears `compound_write_limit`).
    let warm: Vec<u8> = vec![0x3C; 8 * 1024 * 1024];
    let temp = format!(
        "{}/warm.bin{}{}",
        dir,
        cmdr_fs::staging::STAGING_TEMP_MARKER,
        "warm-test"
    );
    vol.write_from_stream(
        Path::new(&temp),
        WriteMode::CreateOrReplace,
        StreamLength::Known(warm.len() as u64),
        Box::new(InlineReadStream::new(warm, None)),
        &|_| std::ops::ControlFlow::Continue(()),
    )
    .await
    .unwrap();

    let data: Vec<u8> = (0..=240u8).cycle().take(4 * 1024 * 1024).collect();
    let size = data.len() as u64;
    assert!(
        vol.write_is_single_shot(StreamLength::Known(size)).await,
        "a warm loopback uplink moves 4 MiB in well under 250 ms"
    );

    let path = format!("{}/promised.bin", dir);
    let (requests_before, compounds_before) = request_counts(&vol).await;
    let written = vol
        .write_from_stream(
            Path::new(&path),
            WriteMode::CreateOrReplace,
            StreamLength::Known(size),
            Box::new(InlineReadStream::new(data.clone(), None)),
            &|_| std::ops::ControlFlow::Continue(()),
        )
        .await
        .unwrap();
    let (requests_after, compounds_after) = request_counts(&vol).await;

    assert_eq!(written, size);
    assert_eq!(
        (compounds_after - compounds_before, requests_after - requests_before),
        (2, 8),
        "the promised write must leave as ONE compound frame (plus the post-write stat)"
    );
    assert_eq!(drain(vol.open_read_stream(Path::new(&path)).await.unwrap()).await, data);

    ensure_clean(&vol, &dir).await;
}

// ── The credit window and the copy-slot clamp ──────────────────

/// Against a real server, the copy-slot count stays inside its two bounds: never
/// above what the user asked for (the detached host's default), and never `0` —
/// a copy engine handed zero slots does nothing at all.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_copy_concurrency_stays_within_the_credit_window() {
    let vol = make_docker_volume().await;
    let requested = vol.inner.host().settings().max_concurrent_operations(BACKEND);
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    // Any op clones the session, which is where the window's capacity is measured.
    let _ = vol.list_directory(Path::new(&dir), None).await.unwrap();

    let slots = vol.max_concurrent_ops();
    assert!(
        (1..=requested).contains(&slots),
        "copy slots must stay in 1..={requested} once the credit window is measured, got {slots}"
    );

    ensure_clean(&vol, &dir).await;
}

// ── A credit window too small for one big frame ────────────────

/// No fixture grants a small window, so `CreditCapProxy` caps the guest
/// fixture's at 64 credits, the way Samba's `smb2 max credits = 64` would. Its
/// `max_read` and `max_write` stay at 8 MiB, which is the dangerous pairing: a
/// 5 MiB READ or WRITE charges 80 credits, plus the CREATE, FLUSH, and CLOSE
/// riding with it, so one compound frame of it can never be funded.
async fn credit_capped_volume(proxy: &credit_cap_proxy::CreditCapProxy) -> SmbVolume {
    let volume_id = cmdr_fs::volume::smb_volume_id("127.0.0.1", proxy.port(), "public");
    let vol = connect_smb_volume(
        "public",
        MountAnchor::at_share_root(TEST_MOUNT_ROOT),
        &volume_id,
        SmbConnectionParams::new("127.0.0.1", "public", proxy.port(), None, None),
        VolumeHost::detached(),
    )
    .await
    .expect("connecting through the credit-cap proxy");
    // A few round trips, so the server has had a chance to decline growing the
    // window and smb2 knows its ceiling.
    let _ = vol.list_directory(Path::new(""), None).await;
    let (_tree, conn) = vol.clone_session().await.unwrap();
    let ceiling = conn.credit_ceiling().expect("smb2 has seen the window stop growing");
    assert!(ceiling <= 64, "the proxy caps the window at 64, smb2 saw {ceiling}");
    vol
}

/// Five MiB of a recognizable pattern, seeded as `{dir}/five-mib.bin` over a
/// direct (uncapped) connection.
async fn seed_five_mib(direct: &SmbVolume, dir: &str) -> (String, Vec<u8>) {
    let data: Vec<u8> = (0..=255u8).cycle().take(5 * 1024 * 1024).collect();
    let path = format!("{}/five-mib.bin", dir);
    direct.create_file(Path::new(&path), &data).await.unwrap();
    (path, data)
}

/// A hinted read the window can't fund in one frame streams from the start:
/// smb2's `quick_read_limit` counts the credit ceiling, so the fast path never
/// sends a READ it can't pay for, even on a warm link whose rate alone would
/// lift the limit to `max_read`. (If the ceiling shrinks between that check and
/// the send, smb2 refuses the READ before it reaches the wire and the
/// `CreditStarvation` arm streams anyway.)
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_hinted_read_the_credit_window_cant_fund_streams_instead() {
    let proxy = credit_cap_proxy::CreditCapProxy::start(guest_port(), 64).await;
    let vol = credit_capped_volume(&proxy).await;
    let direct = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&direct, &dir).await;
    direct.create_directory(Path::new(&dir)).await.unwrap();
    let (path, data) = seed_five_mib(&direct, &dir).await;

    // Warm the link, so the rate alone would say "one READ".
    drain(vol.open_read_stream(Path::new(&path)).await.unwrap()).await;
    let (_tree, conn) = vol.clone_session().await.unwrap();
    assert!(
        conn.quick_read_limit() < data.len() as u64,
        "the credit ceiling must hold the limit under 5 MiB, got {}",
        conn.quick_read_limit()
    );

    let started = std::time::Instant::now();
    let mut stream = vol
        .open_read_stream_with_hint(Path::new(&path), Some(data.len() as u64))
        .await
        .expect("an unfundable compound read must fall through to streaming");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the fallback must be immediate, not a credit wait; took {:?}",
        started.elapsed()
    );
    let mut got = Vec::new();
    let mut chunks = 0;
    while let Some(chunk) = stream.next_chunk().await {
        got.extend_from_slice(&chunk.unwrap());
        chunks += 1;
    }

    assert_eq!(got, data, "the streamed read must serve the file whole");
    assert!(
        chunks > 1,
        "the file must arrive streamed, in chunks; the compound path hands it over as one"
    );
    assert_eq!(
        vol.session_state(),
        ConnectionState::Direct,
        "a window too small for one READ is not a dead connection"
    );

    ensure_clean(&direct, &dir).await;
}

/// A write the window can't fund in one frame gets NO single-shot promise, so
/// the transfer layer stages it, and the staged write streams in chunks the
/// window can carry. Promising it would send a frame smb2 refuses, and the
/// transfer would have nowhere safe to stream the bytes.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_write_the_credit_window_cant_fund_is_staged_and_streams() {
    let proxy = credit_cap_proxy::CreditCapProxy::start(guest_port(), 64).await;
    let vol = credit_capped_volume(&proxy).await;
    let direct = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&direct, &dir).await;
    direct.create_directory(Path::new(&dir)).await.unwrap();
    let data: Vec<u8> = (0..=250u8).cycle().take(5 * 1024 * 1024).collect();
    let size = data.len() as u64;

    assert!(
        size <= vol.negotiated_max_write().await.unwrap(),
        "5 MiB fits one WRITE by size alone; only the credits rule it out"
    );
    assert!(
        !vol.write_is_single_shot(StreamLength::Known(size)).await,
        "a frame the window can't fund is no single shot"
    );
    assert!(
        vol.write_is_single_shot(StreamLength::Known(4096)).await,
        "a small file still fits one frame"
    );

    let temp = format!(
        "{}/five-mib.bin{}{}",
        dir,
        cmdr_fs::staging::STAGING_TEMP_MARKER,
        "credit-test"
    );
    let written = vol
        .write_from_stream(
            Path::new(&temp),
            WriteMode::CreateOrReplace,
            StreamLength::Known(size),
            Box::new(InlineReadStream::new(data.clone(), None)),
            &|_| std::ops::ControlFlow::Continue(()),
        )
        .await
        .expect("a staged write streams through a small window");
    assert_eq!(written, size);
    assert_eq!(
        drain(direct.open_read_stream(Path::new(&temp)).await.unwrap()).await,
        data
    );

    ensure_clean(&direct, &dir).await;
}

/// The one write that must NOT stream when its frame is refused: one to the
/// user's real filename, which only a single-shot promise sends there. If the
/// window shrank after the promise, the frame is refused before it reaches the
/// wire, and streaming instead would leave a partial at that name through a
/// crash. So the write fails, fast, and the name stays empty. Both modes: a
/// promise goes out as `CreateNew` onto a name expected free (the exclusive
/// frame) and as `CreateOrReplace` onto a name the caller claimed.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_refused_frame_to_a_final_name_writes_nothing_there() {
    let proxy = credit_cap_proxy::CreditCapProxy::start(guest_port(), 64).await;
    let vol = credit_capped_volume(&proxy).await;
    let direct = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&direct, &dir).await;
    direct.create_directory(Path::new(&dir)).await.unwrap();

    for mode in [WriteMode::CreateNew, WriteMode::CreateOrReplace] {
        let data = vec![0x5Au8; 5 * 1024 * 1024];
        let final_name = format!("{}/five-mib.bin", dir);
        let started = std::time::Instant::now();
        let result = vol
            .write_from_stream(
                Path::new(&final_name),
                mode,
                StreamLength::Known(data.len() as u64),
                Box::new(InlineReadStream::new(data, None)),
                &|_| std::ops::ControlFlow::Continue(()),
            )
            .await;

        assert!(
            result.is_err(),
            "{mode:?}: a refused one-shot frame must not stream to the real name"
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{mode:?}: smb2 refuses an unfundable frame at once; took {:?}",
            started.elapsed()
        );
        let names: Vec<String> = direct
            .list_directory(Path::new(&dir), None)
            .await
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert!(
            names.is_empty(),
            "{mode:?}: nothing may reach the final name; got {names:?}"
        );
    }

    ensure_clean(&direct, &dir).await;
}

/// A refused frame to a name another writer already holds leaves THEIR file
/// byte for byte, in both modes: smb2 refuses before anything reaches the wire,
/// so neither `CreateNew`'s exclusive CREATE nor `CreateOrReplace`'s truncating
/// one ever runs, and there's no partial for the cleanup to take away.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_refused_frame_to_a_taken_final_name_leaves_the_file_there_whole() {
    let proxy = credit_cap_proxy::CreditCapProxy::start(guest_port(), 64).await;
    let vol = credit_capped_volume(&proxy).await;
    let direct = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&direct, &dir).await;
    direct.create_directory(Path::new(&dir)).await.unwrap();
    let theirs = b"another writer's file".to_vec();
    let final_name = format!("{}/taken.bin", dir);
    direct.create_file(Path::new(&final_name), &theirs).await.unwrap();

    for mode in [WriteMode::CreateNew, WriteMode::CreateOrReplace] {
        let data = vec![0xA5u8; 5 * 1024 * 1024];
        let result = vol
            .write_from_stream(
                Path::new(&final_name),
                mode,
                StreamLength::Known(data.len() as u64),
                Box::new(InlineReadStream::new(data, None)),
                &|_| std::ops::ControlFlow::Continue(()),
            )
            .await;

        assert!(result.is_err(), "{mode:?}: the frame must be refused, got {result:?}");
        assert_eq!(
            drain(direct.open_read_stream(Path::new(&final_name)).await.unwrap()).await,
            theirs,
            "{mode:?}: a refused frame must leave the other writer's file untouched"
        );
    }

    ensure_clean(&direct, &dir).await;
}

/// A staging temp the window can't carry in one frame streams, and the
/// streaming writer honors `WriteMode` the way the frame does: `CreateNew`
/// opens with `FileCreate`, so a taken temp name is refused typed and left
/// alone, while a free one lands every byte.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_create_new_temp_the_window_cant_frame_streams_without_clobbering() {
    let proxy = credit_cap_proxy::CreditCapProxy::start(guest_port(), 64).await;
    let vol = credit_capped_volume(&proxy).await;
    let direct = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&direct, &dir).await;
    direct.create_directory(Path::new(&dir)).await.unwrap();
    let temp_name = |tag: &str| format!("{}/five-mib.bin{}{}", dir, cmdr_fs::staging::STAGING_TEMP_MARKER, tag);
    let (taken, free) = (temp_name("taken"), temp_name("free"));
    direct
        .create_file(Path::new(&taken), b"someone else's temp")
        .await
        .unwrap();
    let data: Vec<u8> = (0..=250u8).cycle().take(5 * 1024 * 1024).collect();
    assert!(
        !vol.write_is_single_shot(StreamLength::Known(data.len() as u64)).await,
        "fixture precondition: 5 MiB must not fit one frame through the capped window"
    );

    cmdr_fs::volume::conformance::assert_write_from_stream_create_new_refuses_to_clobber(
        &vol,
        Path::new(&taken),
        Path::new(&free),
        &data,
    )
    .await;

    ensure_clean(&direct, &dir).await;
}

/// The scan pool's prefetch reads up to a whole `max_read` in one frame. One
/// the window can't fund falls through to streaming on the main session, the
/// same as a file too big for one READ, so enrichment still gets the photo
/// instead of skipping it as unreadable.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_prefetch_the_credit_window_cant_fund_streams_on_the_main_session() {
    let proxy = credit_cap_proxy::CreditCapProxy::start(guest_port(), 64).await;
    let vol = credit_capped_volume(&proxy).await;
    let direct = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&direct, &dir).await;
    direct.create_directory(Path::new(&dir)).await.unwrap();
    let (path, data) = seed_five_mib(&direct, &dir).await;

    vol.open_scan_pool().await;
    assert!(vol.inner.scan_pool.read().await.is_some(), "the pool opened");
    let stream = vol
        .open_read_stream_for_scan_impl(Path::new(&path), Some(data.len() as u64))
        .await
        .expect("an unfundable prefetch must fall through, not fail the file");
    assert_eq!(drain(stream).await, data);
    vol.close_scan_pool().await;

    ensure_clean(&direct, &dir).await;
}

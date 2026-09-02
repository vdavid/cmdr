//! What a byte-path operation COSTS on the wire, against a real server
//! (requires Docker SMB containers).
//!
//! Sibling to the two byte-path suites and a different question from either:
//! they ask whether the bytes are right, this one asks how many frames carried
//! them and how many such operations the server's credit window can carry at
//! once. So the hinted read is here for its ONE compound frame while its
//! size-drift behavior stays in `read_stream_integration_test.rs`, and the
//! single-shot write promise is here (both the wire proof and the
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
/// concurrent 4 MB reads ask for 1,300 credits against a ~512-credit window and
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
    // `(1, 4)`. Same shape as the write cell below.
    assert_eq!(
        (compounds_after - compounds_before, requests_after - requests_before),
        (1, 3),
        "a hinted small read must leave as ONE compound frame carrying CREATE+READ+CLOSE; a 3-RTT streaming open is what this prevents"
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
        vol.write_is_single_shot(size).await,
        "4 KiB fits one WRITE on every SMB2 dialect"
    );

    let smb_path = format!("{}/one-shot.bin", dir);
    let (requests_before, compounds_before) = request_counts(&vol).await;
    let written = vol
        .write_from_stream(
            Path::new(&smb_path),
            size,
            Box::new(InlineReadStream::new(data.clone())),
            &|_, _| std::ops::ControlFlow::Continue(()),
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

/// The other direction against a real server: a file too big for one WRITE gets
/// NO promise, so the transfer layer keeps staging it. ❌ The answer must come
/// from the negotiated `max_write_size`, never from a size the caller picked.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_write_over_the_negotiated_limit_is_not_single_shot() {
    let vol = make_docker_volume().await;
    let max_write = vol
        .negotiated_max_write()
        .await
        .expect("a connected volume has negotiated params");

    assert!(vol.write_is_single_shot(max_write).await, "the limit itself fits");
    assert!(
        !vol.write_is_single_shot(max_write + 1).await,
        "one byte over needs a second WRITE, so the write is no longer all-or-nothing"
    );
    assert!(
        !vol.write_is_single_shot(0).await,
        "an empty file has no WRITE to compound with; it takes the streaming writer"
    );
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

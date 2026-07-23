//! Integration tests for media enrichment's direct-session byte fetch (plan M1):
//! `VolumeByteFetcher` reading image bytes through a REAL `SmbVolume`'s smb2
//! session — the read path that replaces `std::fs` on the OS mount (and with it
//! the macOS TCC "network volumes" failure mode) for Direct-connected shares.
//!
//! Every test here is `#[ignore]`d so default runs skip it. Start the containers
//! with `./apps/desktop/test/smb-servers/start.sh`, then run
//! `cargo nextest run smb_integration --run-ignored all`. Declared as a
//! `#[cfg(test)]` submodule of `smb` alongside `smb_integration_test`; shared
//! helpers come from `super::smb_test_support`.

use super::smb_test_support::*;
use super::*;

use crate::media_index::network::fetch::{ByteFetcher, FetchError, VolumeByteFetcher, os_join};

/// End-to-end proof of the M1 read path: bytes written to a real SMB share come
/// back byte-for-byte through `VolumeByteFetcher`, called exactly the way the
/// enrichment pass calls it — from a blocking thread, with the OS-JOINED path
/// (mount root + index-relative path) and the index's size as the hint. Runs the
/// fetch twice: with the true size hint (SMB's compound fast-path for small
/// files) and with no hint (the streaming path), so both transports are proven.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_media_fetch_reads_bytes_via_direct_session() {
    let vol = std::sync::Arc::new(make_docker_volume().await);
    let mount_root = vol.root().to_string_lossy().into_owned();

    // A unique file so parallel runs never collide; content big enough to span
    // several chunks on the streaming path.
    let name = format!("{}.jpg", test_dir_name());
    let content: Vec<u8> = (0..192 * 1024).map(|i| (i % 251) as u8).collect();
    vol.create_file(Path::new(&name), &content)
        .await
        .expect("seed the share");

    let fetcher = VolumeByteFetcher::new(vol.clone(), tokio::runtime::Handle::current());
    let os_path = os_join(&mount_root, &format!("/{name}"));
    let size = content.len() as u64;

    // The enrichment pass fetches from a blocking thread, never a runtime worker.
    let (with_hint, without_hint) = tokio::task::spawn_blocking(move || {
        let with_hint = fetcher.fetch(&os_path, Some(size), std::time::Duration::from_secs(30));
        let without_hint = fetcher.fetch(&os_path, None, std::time::Duration::from_secs(30));
        (with_hint, without_hint)
    })
    .await
    .expect("blocking task");

    assert_eq!(
        with_hint.expect("hinted fetch"),
        content,
        "hinted fetch must return the exact bytes"
    );
    assert_eq!(
        without_hint.expect("unhinted fetch"),
        content,
        "unhinted (streaming) fetch must return the exact bytes"
    );

    vol.delete(Path::new(&name)).await.expect("cleanup");
}

/// Typed classification over the real transport: a missing file is `NotFound`
/// (skip; GC collects it) — never a disconnect that would pause the whole pass.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_media_fetch_missing_file_is_not_found() {
    let vol = std::sync::Arc::new(make_docker_volume().await);
    let mount_root = vol.root().to_string_lossy().into_owned();
    let fetcher = VolumeByteFetcher::new(vol.clone(), tokio::runtime::Handle::current());
    let os_path = os_join(&mount_root, &format!("/{}-missing.jpg", test_dir_name()));

    let err = tokio::task::spawn_blocking(move || {
        fetcher.fetch(&os_path, Some(10), std::time::Duration::from_secs(30))
    })
    .await
    .expect("blocking task")
    .expect_err("a missing file must error");
    assert!(
        matches!(err, FetchError::NotFound),
        "a vanished source is NotFound, got {err:?}"
    );
}

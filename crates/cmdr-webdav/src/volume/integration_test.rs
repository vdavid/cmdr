//! What only a real server can answer: the auth schemes, the listing and byte
//! surfaces, and the write path's refusals, against Apache `mod_dav`.
//!
//! ❗ **Every `#[ignore]`d test in this crate is a Docker cell**, by
//! construction: the integration lane runs `--run-ignored only` over the whole
//! package, so anything ignored here runs in CI whatever it's called. A
//! measurement that must NOT gate CI needs its own env gate rather than an
//! `#[ignore]`.
//!
//! The servers, the ports, and what each one is for:
//! `apps/desktop/test/webdav-servers/README.md`.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::credentials::InMemoryCredentials;
use cmdr_fs::volume::{Volume, VolumeError, VolumeReadStream};
use tokio_util::sync::CancellationToken;

use super::testing::*;
use super::{WebdavVolume, connect_webdav_volume};
use crate::{WebdavConnectError, WebdavConnectionParams};

const FIXTURE: &str = "webdav-servers/start.sh (webdav-fixture)";

// ── Helpers ──────────────────────────────────────────────────────────

/// Dials `service` with `secret` in the store (or nothing), under a volume id of
/// this cell's own, and hands back whatever the connect answered.
async fn dial(
    service: &str,
    fallback_port: u16,
    secret: Option<&str>,
    cell: &str,
) -> Result<WebdavVolume, WebdavConnectError> {
    let params = WebdavConnectionParams::new(fixture_base_url(service, fallback_port), FIXTURE_USER, FIXTURE_ROOT);
    let credentials = match secret {
        Some(secret) => InMemoryCredentials::new().with_entry(
            &params.credential_service(),
            Some(&params.username),
            &params.username,
            secret,
        ),
        None => InMemoryCredentials::new(),
    };
    let host = VolumeHost::builder().credentials(Arc::new(credentials)).build();
    connect_webdav_volume(
        "fixture",
        &format!("webdav-test-{cell}"),
        params,
        host,
        CancellationToken::new(),
    )
    .await
}

/// The stock server plus a scratch directory of this cell's own.
async fn stock_with_scratch() -> (WebdavVolume, PathBuf) {
    let volume = connect_fixture("APACHE", 13480).await;
    let dir = scratch_dir(&volume).await;
    (volume, dir)
}

/// Removes everything a cell built, deepest first, and the scratch dir itself.
async fn clean(volume: &WebdavVolume, dir: &Path) {
    fn remove<'a>(volume: &'a WebdavVolume, path: &'a Path) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            if let Ok(entries) = volume.list_directory(path, None).await {
                for entry in entries {
                    remove(volume, &path.join(&entry.name)).await;
                }
            }
            let _ = volume.delete(path).await;
        })
    }
    remove(volume, dir).await;
}

/// Reads a whole file back, the way the copy path does.
async fn read_whole(volume: &WebdavVolume, path: &Path) -> Vec<u8> {
    let mut stream = volume.open_read_stream(path).await.expect(FIXTURE);
    let mut out = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        out.extend_from_slice(&chunk.expect(FIXTURE));
    }
    out
}

/// A source stream over a buffer, handed over in pieces that don't line up with
/// anything: a source's chunk size is its own business.
struct BufferSource {
    bytes: Vec<u8>,
    at: usize,
}

impl VolumeReadStream for BufferSource {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.at >= self.bytes.len() {
                return None;
            }
            let end = (self.at + 70_001).min(self.bytes.len());
            let chunk = self.bytes[self.at..end].to_vec();
            self.at = end;
            Some(Ok(chunk))
        })
    }
    fn total_size(&self) -> u64 {
        self.bytes.len() as u64
    }
    fn bytes_read(&self) -> u64 {
        self.at as u64
    }
}

fn source(bytes: Vec<u8>) -> Box<dyn VolumeReadStream> {
    Box::new(BufferSource { bytes, at: 0 })
}

async fn write(volume: &WebdavVolume, path: &Path, bytes: Vec<u8>) -> Result<u64, VolumeError> {
    let size = bytes.len() as u64;
    volume
        .write_from_stream(path, size, source(bytes), &|_, _| ControlFlow::Continue(()))
        .await
}

// ── The listing surface ──────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn the_root_listing_tells_files_from_directories_and_knows_sizes() {
    let volume = connect_fixture("APACHE", 13480).await;

    let entries = volume
        .list_directory(Path::new(FIXTURE_ROOT), None)
        .await
        .expect(FIXTURE);
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();

    let hello = entries
        .iter()
        .find(|e| e.name == "hello.txt")
        .unwrap_or_else(|| panic!("the landmark file must be in the root listing, found {names:?}"));
    assert!(!hello.is_directory);
    assert_eq!(hello.size, Some(6), "`hello\\n` is six bytes");

    let large = entries.iter().find(|e| e.name == FIXTURE_LARGE_FILE).expect(FIXTURE);
    assert_eq!(large.size, Some(4 * 1024 * 1024));

    for dir in ["docs", "nested", "many", "empty", "photos"] {
        let entry = entries
            .iter()
            .find(|e| e.name == dir)
            .unwrap_or_else(|| panic!("`{dir}/` must be in the root listing, found {names:?}"));
        assert!(entry.is_directory, "`{dir}` is a collection");
    }

    // The collection itself, which PROPFIND `Depth: 1` returns as its first
    // response, is never a pane entry.
    assert!(
        !names
            .iter()
            .any(|n| n.is_empty() || *n == "." || *n == ".." || *n == "dav")
    );

    let many = volume
        .list_directory(&Path::new(FIXTURE_ROOT).join("many"), None)
        .await
        .expect(FIXTURE);
    assert_eq!(many.len(), 300);
    let empty = volume
        .list_directory(&Path::new(FIXTURE_ROOT).join("empty"), None)
        .await
        .expect(FIXTURE);
    assert!(empty.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn a_name_with_spaces_and_utf8_round_trips_through_every_verb() {
    // Names travel percent-encoded in the URL and come back encoded in the
    // multistatus `href`, so a decode missed anywhere shows up as `na%C3%AFve`
    // in a pane or a 404 on a file the listing just showed.
    let (volume, dir) = stock_with_scratch().await;

    let root = volume
        .list_directory(Path::new(FIXTURE_ROOT), None)
        .await
        .expect(FIXTURE);
    assert!(
        root.iter().any(|e| e.name == "naïve name.txt" && !e.is_directory),
        "found {:?}",
        root.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert_eq!(
        read_whole(&volume, &Path::new(FIXTURE_ROOT).join("naïve name.txt")).await,
        b"ok\n"
    );

    let summer = volume
        .list_directory(&Path::new(FIXTURE_ROOT).join("photos/2024 summer"), None)
        .await
        .expect(FIXTURE);
    assert!(summer.iter().any(|e| e.name == "beach.txt"));

    let created = dir.join("résumé draft ü.txt");
    volume.create_file(&created, b"mine").await.expect(FIXTURE);
    assert!(volume.exists(&created).await);
    let renamed = dir.join("final — naïve.txt");
    volume.rename(&created, &renamed, false).await.expect(FIXTURE);
    assert!(!volume.exists(&created).await);
    assert_eq!(read_whole(&volume, &renamed).await, b"mine");

    clean(&volume, &dir).await;
}

// ── The byte surface ─────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn a_whole_file_stream_is_byte_exact_and_knows_its_size_up_front() {
    let volume = connect_fixture("APACHE", 13480).await;

    let mut stream = volume
        .open_read_stream(&Path::new(FIXTURE_ROOT).join(FIXTURE_LARGE_FILE))
        .await
        .expect(FIXTURE);
    // The transfer layer draws its progress bar from `total_size()` before the
    // first chunk lands: that is `Content-Length`, and Apache always sends it.
    let size = stream.total_size();
    assert_eq!(size, 4 * 1024 * 1024);

    let mut read = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        read.extend_from_slice(&chunk.expect(FIXTURE));
    }
    assert_eq!(read.len() as u64, size, "the size promised up front is what arrived");
    assert_eq!(stream.bytes_read(), size);
    assert_same_bytes(&read, &fixture_large_bytes(read.len()), "a whole-file stream");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn a_bounded_range_comes_back_exactly_and_never_over_long() {
    // What remote-archive browsing asks for: a `.zip`'s central directory is a
    // window at the tail, and one byte either side is a wrong answer. A server
    // that ignores `Range` answers 200 with the whole file, which is the
    // over-long case this pins.
    let volume = connect_fixture("APACHE", 13480).await;

    let expected = fixture_large_bytes(2 * 1024 * 1024);
    let range = volume
        .read_range(&Path::new(FIXTURE_ROOT).join(FIXTURE_LARGE_FILE), 1_000_000, 300_000)
        .await
        .expect(FIXTURE);
    assert_eq!(range.len(), 300_000, "exactly the bytes asked for, never more");
    assert_same_bytes(&range, &expected[1_000_000..1_300_000], "a bounded range");
}

// ── The write path ───────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn a_streamed_write_lands_byte_exact_and_leaves_no_temp_sibling() {
    let (volume, dir) = stock_with_scratch().await;
    let path = dir.join("copied.bin");
    let bytes = fixture_large_bytes(1024 * 1024);

    let written = write(&volume, &path, bytes.clone()).await.expect(FIXTURE);

    assert_eq!(written, bytes.len() as u64);
    assert_same_bytes(&read_whole(&volume, &path).await, &bytes, "a streamed write");
    let entries = volume.list_directory(&dir, None).await.expect(FIXTURE);
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["copied.bin"],
        "no `.cmdr-tmp-*` sibling survives a finished write"
    );

    clean(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn create_directory_over_an_existing_one_is_already_exists() {
    // `MKCOL` on an existing collection is a 405, the same code Apache answers a
    // verb it disallows with; only a probe turns it into the typed answer the
    // conflict UI keys on.
    let (volume, dir) = stock_with_scratch().await;
    let album = dir.join("album");
    volume.create_directory(&album).await.expect(FIXTURE);

    let again = volume.create_directory(&album).await;
    assert!(matches!(again, Err(VolumeError::AlreadyExists(_))), "got {again:?}");

    clean(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn a_forceless_rename_onto_an_occupied_name_is_already_exists_and_touches_nothing() {
    let (volume, dir) = stock_with_scratch().await;
    let source = dir.join("source.txt");
    let target = dir.join("target.txt");
    volume.create_file(&source, b"source bytes").await.expect(FIXTURE);
    volume.create_file(&target, b"the user's target").await.expect(FIXTURE);

    let refused = volume.rename(&source, &target, false).await;
    assert!(matches!(refused, Err(VolumeError::AlreadyExists(_))), "got {refused:?}");

    assert_eq!(
        read_whole(&volume, &source).await,
        b"source bytes",
        "the source is untouched"
    );
    assert_eq!(
        read_whole(&volume, &target).await,
        b"the user's target",
        "and so is the target"
    );

    clean(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn a_forced_rename_replaces_the_destination() {
    let (volume, dir) = stock_with_scratch().await;
    let source = dir.join("source.txt");
    let target = dir.join("target.txt");
    volume.create_file(&source, b"the new bytes").await.expect(FIXTURE);
    volume.create_file(&target, b"old").await.expect(FIXTURE);

    volume.rename(&source, &target, true).await.expect(FIXTURE);

    assert!(!volume.exists(&source).await, "a MOVE leaves no source behind");
    assert_eq!(read_whole(&volume, &target).await, b"the new bytes");

    clean(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn delete_refuses_a_directory_that_still_holds_something() {
    // ❗ `DELETE` on a collection is recursive by protocol. The trait's contract
    // is a leaf-only delete, so a backend that forwards the verb unguarded takes
    // a whole tree down on what the caller meant as one `rmdir`.
    let (volume, dir) = stock_with_scratch().await;
    let album = dir.join("album");
    let keep = album.join("keep.txt");
    volume.create_directory(&album).await.expect(FIXTURE);
    volume.create_file(&keep, b"content").await.expect(FIXTURE);

    let refused = volume.delete(&album).await;
    assert!(
        refused.is_err(),
        "a non-empty directory must not be deleted, got {refused:?}"
    );
    assert!(volume.exists(&keep).await, "and its child is still there");
    assert_eq!(read_whole(&volume, &keep).await, b"content");

    // The leaf-first order does work.
    volume.delete(&keep).await.expect(FIXTURE);
    volume.delete(&album).await.expect(FIXTURE);
    assert!(!volume.exists(&album).await);

    clean(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn copy_within_uses_the_servers_copy_verb() {
    // ❗ `NotSupported` here is a FAIL, not a fallback: Apache implements `COPY`,
    // and a same-server copy that streams every byte through the client is the
    // slow path this verb exists to avoid.
    let (volume, dir) = stock_with_scratch().await;
    let from = dir.join("original.bin");
    let to = dir.join("duplicate.bin");
    let bytes = fixture_large_bytes(300_000);
    volume.create_file(&from, &bytes).await.expect(FIXTURE);

    let copied = volume.copy_within(&from, &to, &|_, _| ControlFlow::Continue(())).await;
    assert!(
        !matches!(copied, Err(VolumeError::NotSupported)),
        "Apache answers COPY; the backend must not decline it"
    );
    assert_eq!(copied.expect(FIXTURE), bytes.len() as u64);
    assert_same_bytes(&read_whole(&volume, &to).await, &bytes, "a server-side copy");
    assert_same_bytes(&read_whole(&volume, &from).await, &bytes, "the original after a copy");

    clean(&volume, &dir).await;
}

// ── The auth schemes ─────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn a_digest_only_server_is_a_typed_refusal() {
    // `webdav-fixture-digest` offers no Basic scheme. The right credentials sent
    // the wrong way have to come back as a typed answer, ❌ never as a loop, a
    // transport error, or a password in the clear on a retry.
    let refused = dial("DIGEST", 13481, Some(FIXTURE_PASSWORD), "digest-only").await;
    assert!(
        matches!(
            refused,
            Err(WebdavConnectError::AuthMethodUnsupported | WebdavConnectError::AuthenticationRejected)
        ),
        "got {:?}",
        refused.map(|_| "a volume")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn a_wrong_password_is_authentication_rejected() {
    let refused = dial("APACHE", 13480, Some("not-the-password"), "wrong-password").await;
    assert!(
        matches!(refused, Err(WebdavConnectError::AuthenticationRejected)),
        "got {:?}",
        refused.map(|_| "a volume")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn no_secret_in_the_store_is_needs_credentials() {
    // Nothing to offer is a different answer from an offer refused: one opens
    // the password prompt, the other tells the user the password is wrong.
    let refused = dial("APACHE", 13480, None, "no-secret").await;
    assert!(
        matches!(refused, Err(WebdavConnectError::NeedsCredentials)),
        "got {:?}",
        refused.map(|_| "a volume")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn a_reconnect_against_a_live_server_succeeds_and_keeps_listing() {
    // HTTP has no session to lose, so there is nothing to simulate losing; what
    // a reconnect means here is "re-prove the credentials still work", and that
    // has to leave the volume usable.
    let volume = connect_fixture("APACHE", 13480).await;

    volume.attempt_reconnect().await.expect(FIXTURE);

    assert!(volume.exists(&Path::new(FIXTURE_ROOT).join("hello.txt")).await);
    let entries = volume
        .list_directory(Path::new(FIXTURE_ROOT), None)
        .await
        .expect(FIXTURE);
    assert!(entries.iter().any(|e| e.name == "docs" && e.is_directory));
}

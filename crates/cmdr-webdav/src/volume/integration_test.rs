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

use cmdr_fs::staging::is_staging_temp_name;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::credentials::InMemoryCredentials;
use cmdr_fs::volume::{Volume, VolumeError, VolumeReadStream};
use tokio_util::sync::CancellationToken;

use super::testing::*;
use super::{WebdavVolume, connect_webdav_volume};
use crate::WebdavConnectError;

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
    let params = fixture_target(service, fallback_port, FIXTURE_USER).params();
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
    chunk: usize,
}

impl VolumeReadStream for BufferSource {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.at >= self.bytes.len() {
                return None;
            }
            let end = (self.at + self.chunk).min(self.bytes.len());
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

/// The piece size every source here hands its bytes over in.
///
/// ❗ A number that divides nothing, and the size-mismatch cells lean on it:
/// hyper stops polling a body the moment `Content-Length` is satisfied, so an
/// over-long source is only visible when the overshoot lands INSIDE a piece it
/// did poll. A round number that divided the promised size would hide it.
const SOURCE_CHUNK: usize = 70_001;

fn source(bytes: Vec<u8>) -> Box<dyn VolumeReadStream> {
    Box::new(BufferSource {
        bytes,
        at: 0,
        chunk: SOURCE_CHUNK,
    })
}

/// Fails, naming what it found, when a `.cmdr-tmp-*` staging sibling survived.
///
/// ❗ Matched with `is_staging_temp_name`, which looks anywhere in the name: a
/// staging sibling is `<destination>.cmdr-tmp-<id>`, so a `starts_with` test
/// passes whatever is in the directory.
async fn assert_no_staging_leftovers(volume: &WebdavVolume, dir: &Path, what: &str) {
    let names: Vec<String> = volume
        .list_directory(dir, None)
        .await
        .expect(FIXTURE)
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert!(
        !names.iter().any(|n| is_staging_temp_name(n)),
        "{what}: a staging sibling outlived the write, found {names:?}"
    );
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
    if not_for_your_own_server("the seeded landmarks (`hello.txt`, `large.bin`, `many/`, `empty/`)") {
        return;
    }
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
    if not_for_your_own_server("the seeded `naïve name.txt` and `photos/2024 summer/`") {
        return;
    }
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
    if not_for_your_own_server("the seeded `large.bin`") {
        return;
    }
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
    if not_for_your_own_server("the seeded `large.bin`") {
        return;
    }
    // What remote-archive browsing asks for: a `.zip`'s central directory is a
    // window at the tail, and one byte either side is a wrong answer. This is
    // the 206 half, against a server that honours the header; the two cells
    // below ask the same of one that doesn't.
    let volume = connect_fixture("APACHE", 13480).await;

    let expected = fixture_large_bytes(2 * 1024 * 1024);
    let range = volume
        .read_range(&Path::new(FIXTURE_ROOT).join(FIXTURE_LARGE_FILE), 1_000_000, 300_000)
        .await
        .expect(FIXTURE);
    assert_eq!(range.len(), 300_000, "exactly the bytes asked for, never more");
    assert_same_bytes(&range, &expected[1_000_000..1_300_000], "a bounded range");
}

// ── A server that ignores `Range` ────────────────────────────────────
//
// ❗ `webdav-fixture-norange` is the stock export behind `MaxRanges none`, so
// every ranged GET comes back 200 with the whole file. RFC 9110 § 14.2 makes
// ranges optional, and `streams.rs` answers that by skipping `offset` bytes
// locally instead of trusting the response as a window. These two cells are
// what run that skip: without a server that ignores the header, it is data-path
// code on every resumed transfer that nothing executes.

/// The status and length a fixture server answers a ranged GET with, asked
/// straight rather than through this backend.
///
/// ❗ Deliberate, and only the two cells below do it: whether the server really
/// ignored `Range` is the premise every other assertion there rests on, and no
/// `Volume` method reports a status code.
async fn raw_ranged_get(service: &str, port: u16, at: &str, range: &str) -> (reqwest::StatusCode, Option<u64>) {
    let target = fixture_target(service, port, FIXTURE_USER);
    let url = target
        .base_url
        .join(at)
        .unwrap_or_else(|e| panic!("joining {at} onto the fixture base URL: {e}"));
    let response = reqwest::Client::builder()
        .user_agent("Cmdr")
        .build()
        .expect("a client with no TLS options is infallible")
        .get(url)
        .basic_auth(&target.username, Some(&target.password))
        .header(reqwest::header::RANGE, range.to_string())
        .send()
        .await
        .expect(FIXTURE);
    (response.status(), response.content_length())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn a_bounded_range_is_exact_even_when_the_server_ignores_the_header() {
    if not_for_your_own_server("`webdav-fixture-norange`, a server that answers 200 to a ranged GET") {
        return;
    }
    // The premise first: this server has to be answering 200 with all 4 MiB,
    // or the cell below it is quietly re-testing the 206 path.
    let (status, length) = raw_ranged_get("NORANGE", 13483, FIXTURE_LARGE_FILE, "bytes=1000000-1299999").await;
    assert_eq!(
        status,
        reqwest::StatusCode::OK,
        "`MaxRanges none` is what makes this server ignore `Range`; it just answered {status} instead"
    );
    assert_eq!(
        length,
        Some(4 * 1024 * 1024),
        "a server that ignored the header sends the WHOLE file"
    );

    let volume = connect_fixture("NORANGE", 13483).await;

    let range = volume
        .read_range(&Path::new(FIXTURE_ROOT).join(FIXTURE_LARGE_FILE), 1_000_000, 300_000)
        .await
        .expect(FIXTURE);

    // ❗ Both halves matter. The length says `read_range` stopped at `len`
    // rather than handing a caller the 4 MiB the server sent, and the bytes say
    // it skipped to the window the caller asked for rather than returning the
    // head of the file.
    assert_eq!(range.len(), 300_000, "exactly the bytes asked for, never more");
    assert_same_bytes(
        &range,
        &fixture_large_bytes(1_300_000)[1_000_000..],
        "a bounded range from a server that ignored it",
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn a_resumed_stream_skips_locally_when_the_server_ignores_the_header() {
    if not_for_your_own_server("`webdav-fixture-norange`, a server that answers 200 to a ranged GET") {
        return;
    }
    // The resume path: every interrupted transfer picks up at an offset, and on
    // this server that offset is honoured by the client or not at all. A stream
    // that trusted the 200 would hand the transfer the head of the file and
    // write it over the bytes already on disk.
    let (status, _) = raw_ranged_get("NORANGE", 13483, FIXTURE_LARGE_FILE, "bytes=3000000-").await;
    assert_eq!(
        status,
        reqwest::StatusCode::OK,
        "the premise: this server ignores `Range`"
    );

    let volume = connect_fixture("NORANGE", 13483).await;
    let offset = 3_000_000;
    let whole = 4 * 1024 * 1024;

    let mut stream = volume
        .open_read_stream_at_offset(&Path::new(FIXTURE_ROOT).join(FIXTURE_LARGE_FILE), offset)
        .await
        .expect(FIXTURE);

    // The progress bar's denominator, which is the FILE's size on both answers:
    // a 206 reads it off `Content-Range`, a 200 off `Content-Length`.
    assert_eq!(stream.total_size(), whole);
    let mut read = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        read.extend_from_slice(&chunk.expect(FIXTURE));
    }
    assert_eq!(
        read.len() as u64,
        whole - offset,
        "a resumed stream yields the tail, never the whole file the server sent"
    );
    assert_eq!(
        stream.bytes_read(),
        whole - offset,
        "and counts only what it handed over"
    );
    assert_same_bytes(
        &read,
        &fixture_large_bytes(whole as usize)[offset as usize..],
        "a resumed stream from a server that ignored `Range`",
    );
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
async fn a_source_that_ends_early_never_reaches_the_users_filename() {
    // ❗ A data-safety cell. `size` is what `Content-Length` promised, and a
    // source that stops short of it ends the request from OUR side; the server
    // has a shorter file staged and would keep it. The refusal has to be typed
    // as ours, the temp has to go, and the destination must never appear.
    //
    // What makes this happen in the wild: `size` comes off a stat, and the file
    // shrinks between that stat and the read.
    let (volume, dir) = stock_with_scratch().await;
    let path = dir.join("short.bin");
    let bytes = fixture_large_bytes(120_000);
    let promised = bytes.len() as u64 + 40_000;

    let refused = volume
        .write_from_stream(&path, promised, source(bytes), &|_, _| ControlFlow::Continue(()))
        .await;

    assert!(
        matches!(refused, Err(VolumeError::IoError { .. })),
        "a short source is the caller's wrong `size`, reported as an I/O refusal; got {refused:?}"
    );
    // ❗ Never `DeviceDisconnected`: a short source ends the body from our side,
    // which `reqwest` reports with the same predicate a dropped connection
    // answers, and reading it that way would take the whole volume offline over
    // one stale size.
    assert_eq!(
        volume.inner.connection_state(),
        super::ConnectionState::Connected,
        "one wrong `size` must not flip the volume offline"
    );
    assert!(!volume.exists(&path).await, "the destination was never written");
    assert_no_staging_leftovers(&volume, &dir, "a source that ended early").await;

    clean(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn a_source_that_overruns_its_promise_never_reaches_the_users_filename() {
    // ❗ The other half, and the sharper one: hyper TRUNCATES a body longer than
    // `Content-Length` rather than refusing it, and the server stores that
    // prefix and answers 201 Created. Nothing on the wire is wrong, so only the
    // byte count this side counted says the file is short. MOVE it and the user
    // has a truncated file wearing the name they asked for, with no error
    // anywhere.
    //
    // What makes this happen in the wild: the file GREW between the stat and
    // the read.
    let (volume, dir) = stock_with_scratch().await;
    let path = dir.join("overrun.bin");
    let bytes = fixture_large_bytes(200_000);
    let promised = 150_000;

    let refused = volume
        .write_from_stream(&path, promised, source(bytes), &|_, _| ControlFlow::Continue(()))
        .await;

    assert!(
        matches!(refused, Err(VolumeError::IoError { .. })),
        "the PUT succeeded and only the count says the body was cut; got {refused:?}"
    );
    assert!(
        !volume.exists(&path).await,
        "a truncated body must never wear the user's filename"
    );
    assert_no_staging_leftovers(&volume, &dir, "a source that overran its promise").await;

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
    if not_for_your_own_server("`webdav-fixture-digest`, a server that offers no Basic scheme") {
        return;
    }
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
    if not_for_your_own_server("the seeded `hello.txt` and `docs/`") {
        return;
    }
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

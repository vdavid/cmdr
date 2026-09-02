//! What only a REAL server can answer, against a real Nextcloud (sabre/dav).
//!
//! Three claims in `DETAILS.md` used to come from reading RFCs rather than from
//! watching a server, and each one shapes code: the `Range` answer decides
//! whether `streams.rs` ever has to skip bytes locally, the chunked-PUT answer
//! is why `writes.rs` always sends `Content-Length`, and the RFC 4331 answer
//! decides whether the free-space indicator has anything to show. Apache
//! `mod_dav` can settle none of them: it honours `Range` natively and omits the
//! quota properties entirely.
//!
//! ❗ **This module is selected by its PATH.** `desktop-rust-webdav-nextcloud`
//! runs `test(volume::nextcloud_test::)` and the shared fixture lane subtracts
//! the same atom, so renaming or moving this module silently takes these cells
//! out of both. `WebdavNextcloudTestAtom` is the one place to change with it.
//!
//! The server is not in the stack's `core` mode: `./start.sh nextcloud`, or
//! `pnpm check webdav-nextcloud`, is what brings it up.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use cmdr_fs::staging::is_staging_temp_name;
use cmdr_fs::volume::{Volume, VolumeError, VolumeReadStream};
use reqwest::header::{CONTENT_TYPE, RANGE};
use reqwest::{Body, Method, StatusCode};

use super::WebdavVolume;
use super::testing::*;

const FIXTURE: &str = "webdav-servers/start.sh nextcloud (webdav-fixture-nextcloud)";

/// The sabre/dav server, plus a scratch directory of this cell's own.
async fn nextcloud_with_scratch() -> (WebdavVolume, PathBuf) {
    let volume = connect_fixture("NEXTCLOUD", 13482).await;
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

/// A request straight at the server, past this backend.
///
/// ❗ Deliberate, and only two cells do it: they ask what the SERVER answers to
/// a request the backend never sends (an unbounded `Range`, a body of unknown
/// length). No `Volume` method can express "send it the wrong way on purpose",
/// and the wire answer is exactly what the claims in `DETAILS.md` are about.
fn raw(method: Method, at: &str) -> reqwest::RequestBuilder {
    let target = fixture_target("NEXTCLOUD", 13482, FIXTURE_USER);
    let url = target
        .base_url
        .join(at)
        .unwrap_or_else(|e| panic!("joining {at} onto the fixture base URL: {e}"));
    reqwest::Client::builder()
        .user_agent("Cmdr")
        .build()
        .expect("a client with no TLS options is infallible")
        .request(method, url)
        .basic_auth(&target.username, Some(&target.password))
}

/// A source stream over a buffer, in pieces that line up with nothing.
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

/// Reads a whole file back, the way the copy path does.
async fn read_whole(volume: &WebdavVolume, path: &Path) -> Vec<u8> {
    let mut stream = volume.open_read_stream(path).await.expect(FIXTURE);
    let mut out = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        out.extend_from_slice(&chunk.expect(FIXTURE));
    }
    out
}

// ── The write path, on a server that isn't Apache ────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the Nextcloud WebDAV fixture: apps/desktop/test/webdav-servers/start.sh nextcloud (webdav-fixture-nextcloud)"]
async fn the_staged_write_path_lands_a_file_byte_exact_on_sabre_dav() {
    // The cell that says this backend speaks to a real server at all: MKCOL,
    // the staged PUT onto a `.cmdr-tmp-*` sibling, the MOVE that renames it, and
    // a read back. Everything else here asks a narrower question.
    let (volume, dir) = nextcloud_with_scratch().await;
    let path = dir.join("copied.bin");
    let bytes = fixture_large_bytes(1024 * 1024);

    let written = volume
        .write_from_stream(
            &path,
            bytes.len() as u64,
            Box::new(BufferSource {
                bytes: bytes.clone(),
                at: 0,
            }),
            &|_, _| ControlFlow::Continue(()),
        )
        .await
        .expect(FIXTURE);

    assert_eq!(written, bytes.len() as u64);
    assert_same_bytes(&read_whole(&volume, &path).await, &bytes, "a staged write to sabre/dav");
    let siblings = volume.list_directory(&dir, None).await.expect(FIXTURE);
    // ❗ `is_staging_temp_name` looks anywhere in the name: a staging sibling is
    // `<destination>.cmdr-tmp-<id>`, so a `starts_with` test would pass whatever
    // the directory holds.
    assert!(
        !siblings.iter().any(|e| is_staging_temp_name(&e.name)),
        "the staging sibling has to be gone, found {:?}",
        siblings.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    clean(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the Nextcloud WebDAV fixture: apps/desktop/test/webdav-servers/start.sh nextcloud (webdav-fixture-nextcloud)"]
async fn a_put_with_no_content_length_is_accepted_rather_than_refused() {
    // ❗ THE reason `writes.rs` sends `Content-Length` from `size`: sabre/dav
    // (Nextcloud, ownCloud) was believed to answer 411 to a body of unknown
    // length. This cell is what says whether that is true of the server we can
    // actually put in front of it. `Body::wrap_stream` has no length, so hyper
    // sends `Transfer-Encoding: chunked` and no `Content-Length` at all.
    //
    // A 411 here would NOT be a failure of this backend; it would confirm the
    // claim. What the assertion pins is the answer, whichever it is, so a
    // change in it is news rather than a silent drift under a comment.
    let (volume, dir) = nextcloud_with_scratch().await;
    let name = "unknown-length.bin";
    let bytes = fixture_large_bytes(256 * 1024);
    let body = Body::wrap_stream(futures_util::stream::iter(
        bytes
            .chunks(64 * 1024)
            .map(|c| Ok::<_, std::io::Error>(c.to_vec()))
            .collect::<Vec<_>>(),
    ));

    let response = raw(
        Method::PUT,
        &format!("{}/{name}", dir.display().to_string().trim_start_matches('/')),
    )
    .header(CONTENT_TYPE, "application/octet-stream")
    .body(body)
    .send()
    .await
    .expect(FIXTURE);
    let status = response.status();

    assert_ne!(
        status,
        StatusCode::LENGTH_REQUIRED,
        "the server refused a body of unknown length with 411, which is what `writes.rs` sends `Content-Length` to avoid; \
         the claim in DETAILS.md is confirmed and its evidence anchor is out of date"
    );
    assert!(
        status.is_success(),
        "a chunked PUT came back {status}; DETAILS.md records what this server answers, and it just changed"
    );
    assert_same_bytes(
        &read_whole(&volume, &dir.join(name)).await,
        &bytes,
        "a chunked PUT's body",
    );

    clean(&volume, &dir).await;
}

// ── The byte surface ─────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the Nextcloud WebDAV fixture: apps/desktop/test/webdav-servers/start.sh nextcloud (webdav-fixture-nextcloud)"]
async fn a_ranged_get_is_answered_with_a_window_rather_than_the_whole_file() {
    // `streams.rs` treats a 200 to a ranged GET as "the server ignored `Range`"
    // and skips `offset` bytes locally. That path costs a whole file's bandwidth
    // when it fires, so whether a real server ever makes it fire is worth
    // knowing. This cell reads the status directly, then checks the backend gets
    // the same bytes either way.
    let (volume, dir) = nextcloud_with_scratch().await;
    let name = "windowed.bin";
    let bytes = fixture_large_bytes(512 * 1024);
    volume
        .write_from_stream(
            &dir.join(name),
            bytes.len() as u64,
            Box::new(BufferSource {
                bytes: bytes.clone(),
                at: 0,
            }),
            &|_, _| ControlFlow::Continue(()),
        )
        .await
        .expect(FIXTURE);

    let at = format!("{}/{name}", dir.display().to_string().trim_start_matches('/'));
    let response = raw(Method::GET, &at)
        .header(RANGE, "bytes=100000-199999")
        .send()
        .await
        .expect(FIXTURE);
    let status = response.status();
    let body = response.bytes().await.expect(FIXTURE);

    assert_eq!(
        status,
        StatusCode::PARTIAL_CONTENT,
        "this server answered {status} to a ranged GET; DETAILS.md records what it answers, and it just changed"
    );
    assert_same_bytes(&body, &bytes[100_000..200_000], "a 206 window");

    // And the backend's own window, which is what a remote archive's central
    // directory is read through.
    let through_the_backend = volume
        .read_range(&dir.join(name), 100_000, 100_000)
        .await
        .expect(FIXTURE);
    assert_same_bytes(
        &through_the_backend,
        &bytes[100_000..200_000],
        "read_range on sabre/dav",
    );

    clean(&volume, &dir).await;
}

// ── RFC 4331 quota ───────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the Nextcloud WebDAV fixture: apps/desktop/test/webdav-servers/start.sh nextcloud (webdav-fixture-nextcloud)"]
async fn quota_reports_the_accounts_own_numbers_not_the_servers_disk() {
    if not_for_your_own_server("the fixture account's exact 5 GiB quota") {
        return;
    }
    // ❗ The question the free-space indicator turns on. `quota-available-bytes`
    // plus `quota-used-bytes` adding up to the account's 5 GiB rather than to
    // the container's disk (tens of GB) is the whole difference between showing
    // a user their own headroom and showing them the host's.
    let volume = connect_fixture("NEXTCLOUD", 13482).await;

    let space = volume.get_space_info().await.expect(FIXTURE);

    assert_eq!(
        space.total_bytes, FIXTURE_NEXTCLOUD_QUOTA_BYTES,
        "the total has to be the ACCOUNT's quota; the container's disk is nothing like this number"
    );
    assert_eq!(
        space.available_bytes + space.used_bytes,
        space.total_bytes,
        "`total` is built from the two RFC 4331 numbers, so it can't disagree with them"
    );
    assert!(
        space.used_bytes > 0,
        "a freshly installed account holds its skeleton files"
    );
    assert!(space.available_bytes < space.total_bytes);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the Nextcloud WebDAV fixture: apps/desktop/test/webdav-servers/start.sh nextcloud (webdav-fixture-nextcloud)"]
async fn an_account_with_no_quota_reports_no_free_space_at_all() {
    if not_for_your_own_server("the fixture's second, quota-less account") {
        return;
    }
    // ❗ The DEFAULT state of a real Nextcloud account, and the reason
    // `get_space_info` tests both numbers for being non-negative: an unlimited
    // account answers `quota-available-bytes` with a negative sentinel, and
    // reading that as a size would put a nonsense figure under the user's pane.
    // `NotSupported` is what makes the indicator show nothing instead.
    let volume = connect_fixture_as("NEXTCLOUD", 13482, FIXTURE_UNLIMITED_USER).await;

    let refused = volume.get_space_info().await;

    assert!(
        matches!(refused, Err(VolumeError::NotSupported)),
        "an unlimited account has no free-space figure to show; got {refused:?}"
    );
}

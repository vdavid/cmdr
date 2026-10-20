//! The upload: one streaming PUT to the path it's handed, which from the
//! transfer engine is always the engine's own staging temp. Only a `CreateNew`
//! write stages here (a PUT to a sibling, then a no-clobber MOVE);
//! `write_from_stream_impl` has why.
//!
//! ❗ `Content-Length` is set from `size` rather than sending a body of unknown
//! length. The size is always known here, so the header costs nothing and takes
//! away every disagreement a proxy or a server configuration could have about a
//! chunked request. (A real Nextcloud accepts a chunked PUT rather than
//! answering 411; `DETAILS.md` § "What a real server answers" carries the
//! observation and its date.) A source that yields a different byte count fails
//! the write, which is reported honestly and what the PUT stored removed.
//!
//! ❗ **The body reads one piece AHEAD**, and that is not a buffering trick: it
//! is what makes the sentence above true. hyper stops POLLING a body the moment
//! `Content-Length` is satisfied, so a source that still had bytes at that
//! moment is invisible unless it was asked before its answer was needed.
//! `DETAILS.md` § "Write staging" carries the two counters this costs and why
//! they mean different things.
//!
//! On a Nextcloud, a file bigger than one chunk goes through its chunked
//! upload instead (`chunked_upload.rs`), each chunk one of these same `PUT`s.
//!
//! ❗ **A pause the server outlasted re-runs the file** ([`PutFailure::CutByPause`]).
//! The engine pauses by parking the source between pieces, which leaves the
//! `PUT` body silent mid-request, and a server drops a silent body (Apache and
//! nginx after about a minute). The source can't be read twice from here, so the
//! cut-off upload is cleaned up, the write waits for the pause to lift, and it
//! answers `ConnectionTimeout`, which the engine's per-file retry runs again from
//! the first byte. `DETAILS.md` § "Pause".

use std::ops::ControlFlow;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use cmdr_fs::error_detail::ErrorDetail;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{CarriedMetadata, ClassOutcome, MetadataClasses, MetadataReport};
use cmdr_fs::pluralize::pluralize_grouped;
use cmdr_fs::staging::temp_sibling_name;
use cmdr_fs::volume::{ScanStop, StreamWritten, VolumeError, VolumeReadStream, WriteMode};
use log::debug;
use reqwest::Method;
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE};
use tokio_util::sync::CancellationToken;

use super::WebdavVolume;
use crate::errors::{Attempted, map_status, map_transport_error};
use crate::transport::{MUTATION_BUDGET, WebdavClient, method};
use cmdr_fs::volume::liveness::Liveness;

/// How often the upload reports progress while the body is on its way.
const PROGRESS_TICK: Duration = Duration::from_millis(200);

/// A staging name nothing else will pick, beside the destination: its name
/// plus the marker every backend's leftover sweep recognizes and a UUID,
/// shortened to fit the server's name limit (`cmdr_fs::staging::names`).
fn staging_sibling(remote: &str) -> String {
    match remote.rsplit_once('/') {
        Some((dir, name)) => format!("{dir}/{}", temp_sibling_name(name)),
        None => temp_sibling_name(remote),
    }
}

/// What the body stream counts, and ❗ these are two different numbers on
/// purpose: the read-ahead means one piece can be out of the source but not yet
/// handed to hyper, and each question wants its own answer.
#[derive(Clone)]
struct BodyCounts {
    /// Bytes pulled OUT of the source. The size guard's number, and the one
    /// `write_from_stream` returns: only this one can see a piece hyper never
    /// asked for, which is the whole point of reading ahead.
    fetched: Arc<AtomicU64>,
    /// Bytes handed TO the body. What progress reports, because it is the same
    /// quantity the progress bar has always shown: a piece counted when it goes
    /// out, never when it is merely in hand. ❗ Reporting `fetched` instead would
    /// run the bar one piece ahead of the wire and invite a user to cancel a
    /// transfer that looked finished.
    handed: Arc<AtomicU64>,
    /// Whether the source reached its OWN end, which tells a short source (its
    /// fault) from a connection cut mid-body (the server's).
    ///
    /// ❗ With the read-ahead this can be true while a piece is still waiting to
    /// go out, which changes nothing: it is only ever read once the request is
    /// over.
    ended: Arc<AtomicBool>,
    /// Whether the body went quiet for a pause: the operation was paused when
    /// the body asked the source for a piece, which is exactly where the
    /// engine's pause parks it. What tells a server cutting off a paused body
    /// from one refusing the upload.
    paused: Arc<AtomicBool>,
}

impl BodyCounts {
    fn new() -> Self {
        Self {
            fetched: Arc::new(AtomicU64::new(0)),
            handed: Arc::new(AtomicU64::new(0)),
            ended: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// The body stream's state: the source, the piece already pulled out of it, and
/// the counters both the guard and the progress bar read.
struct BodySource {
    stream: Box<dyn VolumeReadStream>,
    /// The next piece, already in hand. `None` means either "nothing pulled yet"
    /// (`primed` is false) or "the source is done" (it is true).
    pending: Option<Vec<u8>>,
    primed: bool,
    counts: BodyCounts,
    stop: CancellationToken,
    source_error: Arc<std::sync::Mutex<Option<VolumeError>>>,
    /// The client's silence watch. ❗ A piece handed over counts as the
    /// server being there: hyper asks for the next one only as the socket
    /// drains, and a socket drains only while the far end acknowledges. A
    /// server that answers nothing until the whole upload is in would
    /// otherwise look silent for its entire length.
    liveness: Arc<Liveness>,
    /// The operation's Cancel and pause, read off the source before it moved in.
    pause: ScanStop,
}

impl BodySource {
    /// Pulls one piece into `pending`, counting it as fetched the moment it is
    /// out of the source rather than when it goes on the wire.
    ///
    /// A source failure is recorded for `write_from_stream` to report and
    /// handed back as the `io::Error` that poisons the body.
    async fn fetch(&mut self) -> Result<(), std::io::Error> {
        // ❗ Asked BEFORE the source: the engine's pause parks the source at the
        // start of its `next_chunk`, so a paused operation seen here is a body
        // about to go quiet for as long as the pause lasts.
        if self.pause.is_stopping_or_paused() {
            self.counts.paused.store(true, Ordering::Relaxed);
        }
        match self.stream.next_chunk().await {
            None => {
                self.counts.ended.store(true, Ordering::Relaxed);
                self.pending = None;
            }
            Some(Ok(chunk)) => {
                self.counts.fetched.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                self.pending = Some(chunk);
            }
            Some(Err(e)) => {
                let message = e.to_string();
                *self.source_error.lock_ignore_poison() = Some(e);
                return Err(std::io::Error::other(message));
            }
        }
        Ok(())
    }
}

/// The ownCloud/Nextcloud request header that sets an uploaded file's
/// modification date, in Unix seconds. Not a standard: Nextcloud, ownCloud,
/// and rclone honor it, plain Apache `mod_dav` ignores it, and WebDAV offers
/// nothing standard instead (`getlastmodified` is a protected property no
/// PROPPATCH may set). `DETAILS.md` § "Dates".
const X_OC_MTIME: &str = "X-OC-Mtime";

/// Nextcloud's request header that sets an uploaded file's creation date, in
/// Unix seconds (answered `X-OC-CTime: accepted`). Sent only to a server whose
/// connect probe said it keeps dates (`DETAILS.md` § "Dates").
const X_OC_CTIME: &str = "X-OC-CTime";

/// `X-OC-Mtime`'s value for `modified_at`: whole Unix seconds, truncated.
/// `None` before 1970, which the header can't say.
fn mtime_header_value(modified_at: SystemTime) -> Option<u64> {
    modified_at
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Whether the server says it stored the date `header` carried: Nextcloud
/// answers a PUT that carried `X-OC-Mtime` with `X-OC-MTime: accepted`, and
/// `X-OC-CTime` likewise.
fn accepted(response: &reqwest::Response, header: &str) -> bool {
    response
        .headers()
        .get(header)
        .is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"accepted"))
}

/// The dates an upload carries to the server, read off the source before its
/// first byte, and what the server said about them. They ride the request
/// that gives the file its bytes: the `PUT`, or a chunked upload's assembling
/// `MOVE`.
pub(super) struct UploadDates {
    modified_at: Option<SystemTime>,
    /// Only where the server keeps one (`keeps_dates`).
    created_at: Option<SystemTime>,
}

impl UploadDates {
    pub(super) fn read(stream: &dyn VolumeReadStream, keeps_dates: bool) -> Self {
        Self {
            modified_at: stream.modified_at(),
            created_at: if keeps_dates {
                stream.metadata(MetadataClasses::CREATED).created
            } else {
                None
            },
        }
    }

    /// `request` with the date headers on it.
    pub(super) fn on(&self, mut request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(secs) = self.modified_at.and_then(mtime_header_value) {
            request = request.header(X_OC_MTIME, secs);
        }
        if let Some(secs) = self.created_at.and_then(mtime_header_value) {
            request = request.header(X_OC_CTIME, secs);
        }
        request
    }

    /// What became of the creation date, judged from the answer to the
    /// request that carried it.
    pub(super) fn judge(&self, response: &reqwest::Response, target: &str) -> ClassOutcome {
        if self.modified_at.is_some() && !accepted(response, X_OC_MTIME) {
            // Not a failure: a server without the ownCloud extension (plain
            // `mod_dav`) has no way to store a date, and keeps its own.
            debug!(
                "WebdavVolume::write_from_stream: {target}: the server didn't take X-OC-Mtime, so it keeps its own date",
                target = LogPrivate::file_path(&target)
            );
        }
        if self.created_at.is_none() {
            return ClassOutcome::Absent;
        }
        if accepted(response, X_OC_CTIME) {
            ClassOutcome::Kept
        } else {
            // A server that said at connect it keeps creation dates, then
            // didn't take this one: the write's report carries it to the loss
            // note.
            log::warn!(
                "WebdavVolume::write_from_stream: {target}: the server didn't take X-OC-CTime, so it keeps its own creation date",
                target = LogPrivate::file_path(&target)
            );
            ClassOutcome::Refused
        }
    }

    /// The write's result: `total` bytes, and what the dates came to.
    pub(super) fn written(&self, total: u64, created: ClassOutcome) -> StreamWritten {
        let carried = CarriedMetadata {
            created: self.created_at,
            ..CarriedMetadata::default()
        };
        let report = MetadataReport {
            created,
            ..MetadataReport::ABSENT
        };
        StreamWritten::with_report(total, &report, &carried, self.modified_at)
    }
}

/// Why a streaming `PUT` didn't land.
pub(super) enum PutFailure {
    /// The upload failed, for the reason it carries.
    Refused(VolumeError),
    /// ❗ The server cut off a body a pause left silent. Nothing is wrong with
    /// the file or the server; the bytes sent so far are lost, and the source
    /// can't give them again from here. Carries the operation's pause, to wait
    /// out before the engine runs the file again.
    CutByPause(ScanStop),
}

impl PutFailure {
    /// The error a write answers with, once the caller has cleaned up what the
    /// failure left on the server.
    ///
    /// ❗ A pause cut waits for the pause to lift FIRST, then answers
    /// `ConnectionTimeout`, a transport fault the engine's per-file retry runs
    /// again from the first byte. Answering at once would have the retry start
    /// its new `PUT` straight into the pause, for the server to cut off again.
    /// A cancel while waiting is `Cancelled`, like any other.
    pub(super) async fn settle(self, target: &str, volume_id: &str) -> VolumeError {
        match self {
            Self::Refused(error) => error,
            Self::CutByPause(pause) => {
                log::info!(
                    target: "volume",
                    "WebdavVolume::write_from_stream: {target}: the server cut off a body the pause left silent; it goes again from the first byte once the pause lifts",
                    target = LogPrivate::file_path(&target)
                );
                if pause.should_stop().await {
                    VolumeError::Cancelled(volume_id.to_string().into())
                } else {
                    VolumeError::ConnectionTimeout(target.to_string().into())
                }
            }
        }
    }
}

/// The source yielded a different byte count than `size` promised.
pub(super) fn size_mismatch(remote: &str, got: u64, size: u64) -> VolumeError {
    VolumeError::IoError {
        message: ErrorDetail::path_then(
            remote,
            format!(
                ": the source promised {} and yielded {}",
                pluralize_grouped(size, "byte"),
                pluralize_grouped(got, "byte")
            ),
        ),
        raw_os_error: None,
    }
}

impl WebdavVolume {
    /// Streams `stream` onto `dest`. Returns the bytes written.
    ///
    /// ❗ **`CreateOrReplace` is one plain `PUT` to `dest`**, because from the
    /// transfer layer `dest` is already a staging name: the engine stages every
    /// write to this backend (`write_is_single_shot` keeps its `false` default)
    /// and renames it into place itself, so staging again here would buy
    /// nothing but a second `MOVE` per file and a second temp in the user's
    /// folder. `dest` is a name the caller owns, and ❗ every failure removes
    /// whatever is at it afterwards, the same contract SFTP's truncating open
    /// keeps.
    ///
    /// **`CreateNew` stages here**, on a `.cmdr-tmp-*` sibling MOVEd onto `dest`
    /// with `Overwrite: F`, which the server refuses with 412 when the name is
    /// taken: their file stays, only our temp goes, and a partial never touches
    /// the name. A conditional `PUT` straight to `dest` couldn't promise the
    /// second half: after one fails, nothing says whether what's at the name is
    /// our partial or another writer's file. The transfer engine never asks for
    /// it (it lands its own temp); it's here so the trait's contract holds for
    /// any caller.
    pub(super) async fn write_from_stream_impl(
        &self,
        dest: &Path,
        mode: WriteMode,
        size: u64,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Result<StreamWritten, VolumeError> {
        let remote = self.to_remote_path(dest)?;
        let client = self.clone_client().await?;
        match mode {
            WriteMode::CreateOrReplace => {
                debug!(
                    "WebdavVolume::write_from_stream: {remote}",
                    remote = LogPrivate::file_path(&remote)
                );
                self.put_streaming(&client, &remote, size, stream, on_progress).await
            }
            WriteMode::CreateNew | WriteMode::CreateNewInFreshFolder => {
                let temp = staging_sibling(&remote);
                debug!(
                    "WebdavVolume::write_from_stream: {remote} via {temp}",
                    remote = LogPrivate::file_path(&remote),
                    temp = LogPrivate::file_path(&temp)
                );
                let written = self.put_streaming(&client, &temp, size, stream, on_progress).await?;
                let request = client
                    .request(method("MOVE"), client.url_for(&temp, false))
                    .header("Destination", client.url_for(&remote, false).as_str())
                    .header("Overwrite", "F")
                    .timeout(MUTATION_BUDGET);
                if let Err(e) = self.send(&client, request, &remote, Attempted::TakingAName).await {
                    self.remove_best_effort(&temp).await;
                    return Err(e);
                }
                Ok(written)
            }
        }
    }

    /// Uploads `stream` to `target`, with progress and cancel: one streaming
    /// `PUT`, or on a Nextcloud a file bigger than one chunk goes through its
    /// chunked upload (`chunked_upload.rs`). Returns the bytes written and the
    /// creation date the server refused, if it did; ❗ every failure removes
    /// whatever the upload may have left at `target`, so the caller has
    /// nothing to clean.
    async fn put_streaming(
        &self,
        client: &Arc<WebdavClient>,
        target: &str,
        size: u64,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Result<StreamWritten, VolumeError> {
        // Read before the stream moves into the body. A source knows its dates
        // from its open, so nothing is lost by asking before the first byte.
        let dates = UploadDates::read(&*stream, self.keeps_dates());
        if let Some(uploads) = client.chunked_uploads()
            && size > self.upload_chunk_size()
        {
            return self
                .upload_in_chunks(client, uploads, target, size, stream, &dates, on_progress)
                .await;
        }
        let request = dates.on(client.request(Method::PUT, client.url_for(target, false)));
        match self
            .stream_put(client, request, target, size, stream, on_progress)
            .await
        {
            Ok(response) => Ok(dates.written(size, dates.judge(&response, target))),
            Err(failure) => {
                self.remove_best_effort(target).await;
                Err(failure.settle(target, self.volume_id()).await)
            }
        }
    }

    /// One streaming `PUT` of exactly `size` bytes of `stream`: `request` is the
    /// method and URL plus any headers of the caller's, and this adds the
    /// length and the body. Progress goes to `on_progress` as `(sent, size)`
    /// every tick and once at the end; a `Break` from it cancels.
    ///
    /// `Ok` only for a 2xx after which the source had yielded exactly `size`
    /// bytes. ❗ Cleans up nothing: what a failure leaves on the server is the
    /// caller's, which knows what it was writing, and which then
    /// [`settle`](PutFailure::settle)s the failure.
    pub(super) async fn stream_put(
        &self,
        client: &Arc<WebdavClient>,
        request: reqwest::RequestBuilder,
        target: &str,
        size: u64,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Result<reqwest::Response, PutFailure> {
        let pause = stream.stop_signal();
        let counts = BodyCounts::new();
        let stop = CancellationToken::new();
        let source_error: Arc<std::sync::Mutex<Option<VolumeError>>> = Arc::new(std::sync::Mutex::new(None));
        let body = reqwest::Body::wrap_stream(futures_util::stream::unfold(
            BodySource {
                stream,
                pending: None,
                primed: false,
                counts: counts.clone(),
                stop: stop.clone(),
                source_error: Arc::clone(&source_error),
                liveness: Arc::clone(client.liveness()),
                pause: pause.clone(),
            },
            |mut source| async move {
                // Cancellation is answered before anything is pulled or handed
                // over, so a cancelled upload never puts one more byte on the
                // wire. The piece already read ahead is simply dropped with the
                // state: it was never sent, and what the PUT stored goes either way.
                if source.stop.is_cancelled() {
                    return Some((Err(std::io::Error::other("cancelled")), source));
                }
                if !source.primed {
                    source.primed = true;
                    if let Err(failed) = source.fetch().await {
                        return Some((Err(failed), source));
                    }
                }
                // Nothing in hand after priming means the source is done, which
                // ends the body.
                let chunk = source.pending.take()?;
                // ❗ The read-ahead, and the ONE line the over-long guard rests
                // on: ask the source for its next piece BEFORE handing this one
                // over, so "the source still had bytes" is on record whether or
                // not hyper ever polls again.
                if let Err(failed) = source.fetch().await {
                    return Some((Err(failed), source));
                }
                source.counts.handed.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                source.liveness.heard();
                Some((Ok(Bytes::from(chunk)), source))
            },
        ));
        let request = request
            .header(CONTENT_LENGTH, size)
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(body);

        // The block scopes the in-flight request: leaving it drops the
        // request, which is what aborts a cancelled upload on the wire.
        let outcome = {
            // ❗ `client.send`, not `self.send`: a pause cut is told apart by the
            // raw answer (a 408, or the connection ending), before the status
            // table turns it into a refusal.
            let put = client.send(request);
            let mut put = std::pin::pin!(put);
            let mut tick = tokio::time::interval(PROGRESS_TICK);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    outcome = &mut put => break outcome,
                    _ = tick.tick() => {
                        // ❗ `handed`, and clamped: the bar tracks what went out,
                        // and an over-long source must not push it past 100% on
                        // its way to being refused.
                        let sent = counts.handed.load(Ordering::Relaxed).min(size);
                        if on_progress(sent, size).is_break() {
                            // ❗ Break out NOW rather than wait for the body to be
                            // polled again: a server that stalled mid-upload would
                            // otherwise hold the cancel until it asks for more.
                            stop.cancel();
                            return Err(PutFailure::Refused(VolumeError::Cancelled(self.volume_id().to_string().into())));
                        }
                    }
                }
            }
        };

        // ❗ What came OUT of the source, never what hyper asked for. The two
        // differ by exactly the case this guard exists for.
        let total = counts.fetched.load(Ordering::Relaxed);
        let refused = |error: VolumeError| Err(PutFailure::Refused(error));
        let paused = counts.paused.load(Ordering::Relaxed);
        let response = match outcome {
            Ok(response) if response.status().is_success() => response,
            // ❗ A server that ran out of patience with a body a pause left
            // silent answers 408 while the body is parked (verified on httpd
            // 2.4.69, `webdav-fixture-impatient`, 2026-10-20), or just closes.
            Ok(response) if paused && response.status() == reqwest::StatusCode::REQUEST_TIMEOUT => {
                return Err(PutFailure::CutByPause(pause));
            }
            Ok(response) => return refused(map_status(response.status(), target, Attempted::Reaching)),
            Err(e) => {
                if stop.is_cancelled() {
                    return refused(VolumeError::Cancelled(self.volume_id().to_string().into()));
                }
                if let Some(source) = source_error.lock_ignore_poison().take() {
                    return refused(source);
                }
                if counts.ended.load(Ordering::Relaxed) && total != size {
                    // ❗ A short source ends the request from OUR side (hyper's
                    // `NotEof`), which `reqwest` reports as a request error, the
                    // same predicate a dropped connection answers. ❌ Not the
                    // volume's fault, so not `DeviceDisconnected`: that would flip
                    // the volume offline over one wrong `size`.
                    return refused(size_mismatch(target, total, size));
                }
                if paused && (e.is_request() || e.is_body()) {
                    return Err(PutFailure::CutByPause(pause));
                }
                return refused(map_transport_error(&e, self.volume_id(), target));
            }
        };
        if total != size {
            // ❗ hyper TRUNCATES a body longer than `Content-Length` and the
            // server happily stores the prefix, then answers 201 with nothing
            // wrong anywhere on the wire. Verified on hyper 1.10.1 (its HTTP/1
            // encoder's `Kind::Length` arm), 2026-09-01. Never keep that.
            //
            // ❗ It also stops POLLING once the promise is met, which is why
            // `total` is the read-ahead's count: a source whose pieces divide
            // `size` exactly would otherwise agree with the promise and keep a
            // truncated file.
            return refused(size_mismatch(target, total, size));
        }
        if on_progress(total, size).is_break() {
            return refused(VolumeError::Cancelled(self.volume_id().to_string().into()));
        }
        Ok(response)
    }

    /// Removes what a failed write left at `remote`, and says nothing if that
    /// fails: the error that got us here is the one worth reporting.
    pub(super) async fn remove_best_effort(&self, remote: &str) {
        if let Ok(client) = self.clone_client().await {
            let request = client
                .request(Method::DELETE, client.url_for(remote, false))
                .timeout(MUTATION_BUDGET);
            let _ = self.send(&client, request, remote, Attempted::Reaching).await;
        }
    }
}

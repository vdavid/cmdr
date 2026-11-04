//! The `cmdr-media://` async URI scheme handler: serves the bytes of a token-named
//! file to the viewer's `<img>` / `<embed>`.
//!
//! Registered once in the `tauri::Builder` chain (see `lib.rs`); the registration runs
//! before any window exists, which is correct because `viewer-*` windows are created
//! lazily and inherit the app-wide scheme.
//!
//! Security: the URL carries an unguessable 128-bit token, never a path (see
//! [`super::media`]). The handler resolves the token to a [`super::media::MediaEntry`];
//! an unknown token is a **404**, so there's no way to name an arbitrary file. A
//! deferred token (an image in a Markdown document) first fetches its file under a stall
//! rule ([`fetch_deferred`]), then serves it the same way. A volume token reads by
//! position through its session's source ([`serve_volume`]), under a stall rule too.
//!
//! The handler is a **thin shell over the framework-free answer** (`media_serve.rs`: the
//! token, the `Range` math, the bytes, the outcome), turning a `MediaAnswer` into an HTTP
//! response. It does its OWN `spawn_blocking` + `tokio::time::timeout` (mapping expiry to
//! **504**) rather than reusing `blocking_with_timeout`, because a scheme handler must
//! answer with an HTTP-shaped response, not a typed IPC error. The file-read syscalls
//! carry the same network-mount hazard as IPC commands (`docs/architecture.md` § Platform
//! constraints), only the return shape differs.
//!
//! FDA note: the handler `File::open`s a real path off an IPC-minted token, inheriting
//! the viewer's existing assumption that a viewer only opens after the user picked the
//! file (so FDA is already decided). This is NOT a new pre-gate read path
//! (`fda_gate.rs`); a stray TCC denial reading bytes here is a real access failure, not
//! a scheme bug.

use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::deadline::{StallWatch, blocking_typed_result_until_stalled};
use tauri::http::{Request, Response, StatusCode, header};
use tauri_specta::Event as _;

use super::events::ViewerMediaLoad;
use super::media::{DeferredMedia, MediaEntry, MediaSource, resolve_token};
use super::media_serve::{MediaAnswer, VolumeRead, finish_volume_read, serve_entry};
pub use super::media_serve::{RangeOutcome, ResolvedRange, parse_token_from_uri, resolve_range};
use super::media_volume::{MediaLoad, MediaReadFailure, ReadProgress, ReadWatch, VolumeMedia};

/// The scheme name registered with Tauri. The frontend builds URLs as
/// `cmdr-media://localhost/<token>` (macOS/Linux origin shape).
pub const SCHEME: &str = "cmdr-media";

/// Per-request read timeout. The handler answers 504 if a single media request's
/// blocking file work exceeds this (a hung network mount). Generous because a PDF
/// range request on a slow-but-live disk shouldn't trip it.
const MEDIA_READ_TIMEOUT: Duration = Duration::from_secs(15);

/// The HTTP response for `answer`.
fn respond(answer: MediaAnswer) -> Response<Vec<u8>> {
    match answer {
        MediaAnswer::Body { mime, range, body } => with_body(&mime, &range, body),
        MediaAnswer::Unsatisfiable { total } => unsatisfiable(total),
        MediaAnswer::NotFound => not_found(),
        MediaAnswer::TimedOut => timed_out(),
    }
}

/// Builds the HTTP response for a resolved entry + range, doing the blocking file read.
/// The caller wraps this in `spawn_blocking` + timeout.
fn build_response(entry: &MediaEntry, range_header: Option<String>) -> Response<Vec<u8>> {
    respond(serve_entry(entry, range_header.as_deref()))
}

fn unsatisfiable(total: u64) -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::RANGE_NOT_SATISFIABLE)
        .header(header::CONTENT_RANGE, format!("bytes */{total}"))
        .header(header::ACCEPT_RANGES, "bytes")
        .body(Vec::new())
        .unwrap_or_else(|_| not_found())
}

/// The 200 or 206 for a body read for `range`.
fn with_body(mime: &str, range: &ResolvedRange, body: Vec<u8>) -> Response<Vec<u8>> {
    let status = if range.is_partial {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, body.len().to_string());
    if range.is_partial {
        builder = builder.header(
            header::CONTENT_RANGE,
            format!("bytes {}-{}/{}", range.start, range.end, range.total),
        );
    }
    builder.body(body).unwrap_or_else(|_| not_found())
}

fn not_found() -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Vec::new())
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

fn timed_out() -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::GATEWAY_TIMEOUT)
        .body(Vec::new())
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

/// How long a fetch or a read from a volume may go without progress before its request
/// answers 504. The viewer's pull rule (`commands::file_viewer::PULL_STALL_LIMIT`), for
/// the same reason: above every volume's own read timeout, so a dead connection answers
/// first.
const VOLUME_STALL_LIMIT: Duration = Duration::from_secs(45);

/// Why a deferred fetch produced nothing to serve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FetchFailure {
    /// The bytes can't be had, or aren't what the token may serve: a 404.
    NothingToServe,
    /// No progress for the stall limit: a 504.
    Stalled,
}

impl FetchFailure {
    pub(super) fn response(self) -> Response<Vec<u8>> {
        match self {
            Self::NothingToServe => not_found(),
            Self::Stalled => timed_out(),
        }
    }
}

/// Watches one deferred fetch for [`blocking_typed_result_until_stalled`].
struct DeferredWatch(Arc<dyn DeferredMedia>);

impl StallWatch<FetchFailure> for DeferredWatch {
    fn idle_for(&self) -> Duration {
        self.0.idle_for()
    }

    fn on_poll(&mut self) {}

    fn give_up(&mut self) -> Option<FetchFailure> {
        self.0.give_up();
        Some(FetchFailure::Stalled)
    }
}

/// Fetches a deferred token's bytes on a blocking thread, with no total deadline, only
/// `stall_limit` without progress: a picture on a phone may take a while, and the
/// viewer shows the rest of the document meanwhile.
pub(super) async fn fetch_deferred(
    deferred: Arc<dyn DeferredMedia>,
    stall_limit: Duration,
) -> Result<MediaEntry, FetchFailure> {
    let mut watch = DeferredWatch(Arc::clone(&deferred));
    blocking_typed_result_until_stalled(
        stall_limit,
        &mut watch,
        |_join_failure| FetchFailure::NothingToServe,
        move || deferred.fetch().ok_or(FetchFailure::NothingToServe),
    )
    .await
}

/// Serves `range_header` of a volume token's file: read by position on a blocking thread
/// with no total deadline, only `stall_limit` without a byte, telling `emit` as it goes.
/// A closed session's read answers 404 and tells nobody.
pub(crate) async fn serve_volume(
    media: Arc<VolumeMedia>,
    token: String,
    range_header: Option<String>,
    stall_limit: Duration,
    mut emit: impl FnMut(MediaLoad) + Send,
) -> Response<Vec<u8>> {
    let range = match resolve_range(range_header.as_deref(), media.total_bytes()) {
        RangeOutcome::Unsatisfiable { total } => return unsatisfiable(total),
        RangeOutcome::Serve(range) => range,
    };
    let mime = media.mime().to_string();
    let progress = Arc::new(ReadProgress::new());
    let bytes_total = range.len();
    let mut watch = ReadWatch {
        progress: Arc::clone(&progress),
        bytes_total,
        token: token.clone(),
        emit: &mut emit,
        last_emitted: None,
    };
    let read_progress = Arc::clone(&progress);
    let start = range.start;
    let outcome: Result<Vec<u8>, MediaReadFailure> = blocking_typed_result_until_stalled(
        stall_limit,
        &mut watch,
        |_join_failure| MediaReadFailure::Unreadable,
        move || media.read(start, bytes_total, &read_progress),
    )
    .await;
    let read = VolumeRead {
        token,
        mime,
        range,
        progress: &progress,
    };
    respond(finish_volume_read(outcome, read, emit))
}

/// The Tauri async URI scheme handler. Thin shell: resolve the token, then run the
/// blocking read under its own timeout off the IPC thread, and respond.
///
/// Registered via `register_asynchronous_uri_scheme_protocol(SCHEME, handle_request)`.
pub fn handle_request(
    app: tauri::AppHandle,
    window: String,
    request: Request<Vec<u8>>,
    responder: tauri::UriSchemeResponder,
) {
    let token = parse_token_from_uri(request.uri().path()).map(str::to_string);
    let range_header = request
        .headers()
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let Some(token) = token else {
        responder.respond(not_found());
        return;
    };
    let Some(source) = resolve_token(&token) else {
        // Unknown token: never existed or already dropped (session closed).
        responder.respond(not_found());
        return;
    };

    // Own `spawn_blocking` + timeout: a scheme handler must answer with an HTTP
    // response, so it can't reuse the command layer's timeout helpers.
    tauri::async_runtime::spawn(async move {
        let entry = match source {
            MediaSource::Volume(media) => {
                let emit = move |load: MediaLoad| {
                    // Best-effort: a window that closed mid-read has no one left to tell.
                    let _ = ViewerMediaLoad::from(load).emit_to(&app, window.as_str());
                };
                responder.respond(serve_volume(media, token, range_header, VOLUME_STALL_LIMIT, emit).await);
                return;
            }
            MediaSource::Ready(entry) => entry,
            MediaSource::Deferred(deferred) => match fetch_deferred(deferred, VOLUME_STALL_LIMIT).await {
                Ok(entry) => entry,
                Err(failure) => {
                    responder.respond(failure.response());
                    return;
                }
            },
        };
        let read = tauri::async_runtime::spawn_blocking(move || build_response(&entry, range_header));
        let response = match tokio::time::timeout(MEDIA_READ_TIMEOUT, read).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(_join_err)) => not_found(),
            Err(_elapsed) => timed_out(),
        };
        responder.respond(response);
    });
}

#[cfg(test)]
mod build_response_tests {
    use super::*;
    use crate::file_viewer::content_kind::ViewerContentKind;
    use std::io::Write;

    fn entry_for(path: std::path::PathBuf, mime: &str) -> MediaEntry {
        MediaEntry {
            canonical_path: path,
            kind: ViewerContentKind::Image,
            mime: mime.to_string(),
        }
    }

    fn write_temp(bytes: &[u8]) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().expect("temp file");
        f.write_all(bytes).expect("write");
        f.flush().expect("flush");
        f
    }

    #[test]
    fn serves_full_file_as_200_with_magic_byte_content_type() {
        let body = b"\xFF\xD8\xFFjpeg-ish-bytes-here";
        let f = write_temp(body);
        let entry = entry_for(f.path().to_path_buf(), "image/jpeg");
        let resp = build_response(&entry, None);
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get(header::CONTENT_TYPE).unwrap(), "image/jpeg");
        assert_eq!(resp.headers().get(header::ACCEPT_RANGES).unwrap(), "bytes");
        assert_eq!(resp.body().as_slice(), body);
    }

    #[test]
    fn serves_range_as_206_with_content_range() {
        let body: Vec<u8> = (0..=255u8).collect();
        let f = write_temp(&body);
        let entry = entry_for(f.path().to_path_buf(), "application/pdf");
        let resp = build_response(&entry, Some("bytes=10-19".to_string()));
        assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(resp.headers().get(header::CONTENT_RANGE).unwrap(), "bytes 10-19/256");
        assert_eq!(resp.body().as_slice(), &body[10..=19]);
    }

    #[test]
    fn unsatisfiable_range_is_416() {
        let f = write_temp(b"short");
        let entry = entry_for(f.path().to_path_buf(), "image/png");
        let resp = build_response(&entry, Some("bytes=9000-9999".to_string()));
        assert_eq!(resp.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(resp.headers().get(header::CONTENT_RANGE).unwrap(), "bytes */5");
    }

    #[test]
    fn missing_file_is_404() {
        let entry = entry_for(std::path::PathBuf::from("/nonexistent/cmdr/media/x.png"), "image/png");
        let resp = build_response(&entry, None);
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}

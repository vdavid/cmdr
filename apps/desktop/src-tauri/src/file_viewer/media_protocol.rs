//! The `cmdr-media://` async URI scheme handler: serves the bytes of a token-named
//! local file to the viewer's `<img>` / `<embed>`.
//!
//! Registered once in the `tauri::Builder` chain (see `lib.rs`); the registration runs
//! before any window exists, which is correct because `viewer-*` windows are created
//! lazily and inherit the app-wide scheme.
//!
//! Security: the URL carries an unguessable 128-bit token, never a path (see
//! [`super::media`]). The handler resolves the token to a [`super::media::MediaEntry`];
//! an unknown token is a **404**, so there's no way to name an arbitrary file.
//!
//! The handler is a **thin shell over pure functions** ([`parse_token_from_uri`],
//! [`resolve_range`], the `Content-Type` from the entry) so the testable logic lives
//! outside the Tauri glue. It does its OWN `spawn_blocking` + `tokio::time::timeout`
//! (mapping expiry to **504**) rather than reusing `blocking_with_timeout`, because a
//! scheme handler must answer with an HTTP-shaped response, not an `IpcError`. The
//! file-read syscalls carry the same network-mount hazard as IPC commands
//! (`docs/architecture.md` § Platform constraints), only the return shape differs.
//!
//! FDA note: the handler `File::open`s a real path off an IPC-minted token, inheriting
//! the viewer's existing assumption that a viewer only opens after the user picked the
//! file (so FDA is already decided). This is NOT a new pre-gate read path
//! (`fda_gate.rs`); a stray TCC denial reading bytes here is a real access failure, not
//! a scheme bug.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

use tauri::http::{Request, Response, StatusCode, header};

use super::media::{MediaEntry, resolve_token};

/// The scheme name registered with Tauri. The frontend builds URLs as
/// `cmdr-media://localhost/<token>` (macOS/Linux origin shape).
pub const SCHEME: &str = "cmdr-media";

/// Per-request read timeout. The handler answers 504 if a single media request's
/// blocking file work exceeds this (a hung network mount). Generous because a PDF
/// range request on a slow-but-live disk shouldn't trip it.
const MEDIA_READ_TIMEOUT: Duration = Duration::from_secs(15);

/// The byte range to serve, computed purely from the `Range` header and the file size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRange {
    /// First byte offset to serve (inclusive).
    pub start: u64,
    /// Last byte offset to serve (inclusive).
    pub end: u64,
    /// Total file size in bytes.
    pub total: u64,
    /// True when the client sent a satisfiable `Range` header (-> 206 + `Content-Range`).
    /// False when there was no range (-> 200, whole file up to the cap).
    pub is_partial: bool,
}

impl ResolvedRange {
    /// Number of bytes this range covers.
    pub fn len(&self) -> u64 {
        self.end - self.start + 1
    }

    pub fn is_empty(&self) -> bool {
        self.total == 0
    }
}

/// Outcome of resolving a `Range` header against a file size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeOutcome {
    /// Serve this range. 200 when `!is_partial`, 206 when `is_partial`.
    Serve(ResolvedRange),
    /// The `Range` header was present but unsatisfiable (start past EOF). The handler
    /// answers `416 Range Not Satisfiable` with `Content-Range: bytes */<total>`.
    Unsatisfiable { total: u64 },
}

/// Extracts the token from a `cmdr-media://localhost/<token>` request URI: the first
/// non-empty path segment. Returns `None` when the path is empty.
pub fn parse_token_from_uri(uri_path: &str) -> Option<&str> {
    uri_path.split('/').find(|seg| !seg.is_empty())
}

/// Resolves the `Range` request header against `total` file size, clamping the end to
/// `total - 1`. Honors only the first `bytes=start-end` spec (a single range; WKWebView
/// never sends multipart ranges for media). Anything it can't parse degrades to a full
/// 200 response, never an error.
///
/// Supported forms:
/// - `bytes=START-END`  -> `[START, min(END, total-1)]`
/// - `bytes=START-`     -> `[START, total-1]`
/// - `bytes=-SUFFIX`    -> the last `SUFFIX` bytes
/// - no/!bytes header   -> full file (200)
pub fn resolve_range(range_header: Option<&str>, total: u64) -> RangeOutcome {
    let full = || {
        RangeOutcome::Serve(ResolvedRange {
            start: 0,
            end: total.saturating_sub(1),
            total,
            is_partial: false,
        })
    };

    let Some(raw) = range_header else {
        return full();
    };
    let Some(spec) = raw.trim().strip_prefix("bytes=") else {
        return full(); // unknown unit: serve whole file
    };
    // Only the first range of a possibly-comma-separated list.
    let spec = spec.split(',').next().unwrap_or("").trim();
    let Some((start_s, end_s)) = spec.split_once('-') else {
        return full();
    };
    let start_s = start_s.trim();
    let end_s = end_s.trim();

    // Empty file: any range is unsatisfiable.
    if total == 0 {
        return RangeOutcome::Unsatisfiable { total };
    }

    let (start, end) = if start_s.is_empty() {
        // Suffix form `bytes=-N`: last N bytes.
        let Ok(suffix) = end_s.parse::<u64>() else {
            return full();
        };
        if suffix == 0 {
            return RangeOutcome::Unsatisfiable { total };
        }
        let len = suffix.min(total);
        (total - len, total - 1)
    } else {
        let Ok(start) = start_s.parse::<u64>() else {
            return full();
        };
        if start >= total {
            return RangeOutcome::Unsatisfiable { total };
        }
        let end = if end_s.is_empty() {
            total - 1
        } else {
            match end_s.parse::<u64>() {
                Ok(e) => e.min(total - 1),
                Err(_) => return full(),
            }
        };
        if end < start {
            return RangeOutcome::Unsatisfiable { total };
        }
        (start, end)
    };

    RangeOutcome::Serve(ResolvedRange {
        start,
        end,
        total,
        is_partial: true,
    })
}

/// Reads `[range.start, range.end]` (inclusive) from `path`. Re-stats the file fresh so
/// the size we serve matches the bytes we read (the file could have changed since the
/// token was minted). Used inside the handler's blocking task.
fn read_byte_range(path: &Path, range: &ResolvedRange) -> std::io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    if range.is_empty() {
        return Ok(Vec::new());
    }
    // Serve the whole requested range, uncapped: a no-range `<img>` GET needs the entire
    // file or the image decodes truncated, and capping a 206 would desync `Content-Range`
    // from the body. The 15s read timeout bounds a runaway read; a guard for pathologically
    // huge images is deferred (see `viewer-media-plan.md` § Phase 3).
    let len = range.len();
    file.seek(SeekFrom::Start(range.start))?;
    let mut buf = vec![0u8; len as usize];
    let mut filled = 0usize;
    while filled < buf.len() {
        match file.read(&mut buf[filled..]) {
            Ok(0) => break, // hit EOF early (file shrank); serve what we have
            Ok(n) => filled += n,
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    buf.truncate(filled);
    Ok(buf)
}

/// Builds the HTTP response for a resolved entry + range, doing the blocking file read.
/// Pure of Tauri types so it's unit-testable against a real temp file. The caller wraps
/// this in `spawn_blocking` + timeout.
fn build_response(entry: &MediaEntry, range_header: Option<String>) -> Response<Vec<u8>> {
    // Re-stat for the authoritative size (network-mount hazard handled by the caller's
    // timeout). If we can't stat, the file vanished -> 404.
    let total = match std::fs::metadata(&entry.canonical_path) {
        Ok(meta) => meta.len(),
        Err(_) => return not_found(),
    };

    match resolve_range(range_header.as_deref(), total) {
        RangeOutcome::Unsatisfiable { total } => Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::CONTENT_RANGE, format!("bytes */{total}"))
            .header(header::ACCEPT_RANGES, "bytes")
            .body(Vec::new())
            .unwrap_or_else(|_| not_found()),
        RangeOutcome::Serve(range) => match read_byte_range(&entry.canonical_path, &range) {
            Ok(body) => {
                let status = if range.is_partial {
                    StatusCode::PARTIAL_CONTENT
                } else {
                    StatusCode::OK
                };
                let mut builder = Response::builder()
                    .status(status)
                    .header(header::CONTENT_TYPE, &entry.mime)
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
            Err(_) => not_found(),
        },
    }
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

/// The Tauri async URI scheme handler. Thin shell: resolve the token, then run the
/// blocking read under its own timeout off the IPC thread, and respond.
///
/// Registered via `register_asynchronous_uri_scheme_protocol(SCHEME, handle_request)`.
pub fn handle_request(request: Request<Vec<u8>>, responder: tauri::UriSchemeResponder) {
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
    let Some(entry) = resolve_token(&token) else {
        // Unknown token: never existed or already dropped (session closed).
        responder.respond(not_found());
        return;
    };

    // Own `spawn_blocking` + timeout: a scheme handler must answer with an HTTP
    // response, so we can't reuse `blocking_with_timeout` (it returns an `IpcError`).
    tauri::async_runtime::spawn(async move {
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

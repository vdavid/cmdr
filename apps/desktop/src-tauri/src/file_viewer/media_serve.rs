//! What a `cmdr-media://` request answers, framework-free: the token in the URL, the
//! `Range` math, the bytes, and the outcome as a [`MediaAnswer`]. The app's scheme handler
//! (`media_protocol.rs`) turns an answer into an HTTP response and runs the reads under its
//! timeouts and stall rule; a host with no blocking threads serves a volume token inline
//! ([`serve_volume_inline`]). One source for both, so a browser's `blob:` gets exactly the
//! bytes the app's `<img>` would.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use super::media::MediaEntry;
use super::media_volume::{MediaLoad, MediaLoadStage, MediaReadFailure, ReadProgress, VolumeMedia};

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

/// What one media request answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaAnswer {
    /// The bytes of `range` (a 200 when it isn't partial, else a 206), served as `mime`.
    Body {
        mime: String,
        range: ResolvedRange,
        body: Vec<u8>,
    },
    /// The `Range` asked for nothing the file has: a 416.
    Unsatisfiable { total: u64 },
    /// No such token, a closed session, or bytes that can't be had: a 404.
    NotFound,
    /// The read went quiet for the stall limit: a 504.
    TimedOut,
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

/// Reads `[range.start, range.end]` (inclusive) from `path`. Used inside the handler's
/// blocking task.
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

/// Serves `range_header` of a token minted over a local file, doing the blocking read.
/// Re-stats the file fresh so the size served matches the bytes read (the file could
/// have changed since the token was minted); a file that vanished is a 404.
pub fn serve_entry(entry: &MediaEntry, range_header: Option<&str>) -> MediaAnswer {
    let total = match std::fs::metadata(&entry.canonical_path) {
        Ok(meta) => meta.len(),
        Err(_) => return MediaAnswer::NotFound,
    };
    match resolve_range(range_header, total) {
        RangeOutcome::Unsatisfiable { total } => MediaAnswer::Unsatisfiable { total },
        RangeOutcome::Serve(range) => match read_byte_range(&entry.canonical_path, &range) {
            Ok(body) => MediaAnswer::Body {
                mime: entry.mime.clone(),
                range,
                body,
            },
            Err(_) => MediaAnswer::NotFound,
        },
    }
}

/// How a volume token's read ended, told to `emit` (`Done` or `Failed`, with the bytes
/// read so far) and answered. A closed session's read answers 404 and tells nobody.
pub(super) fn finish_volume_read(
    outcome: Result<Vec<u8>, MediaReadFailure>,
    read: VolumeRead,
    mut emit: impl FnMut(MediaLoad),
) -> MediaAnswer {
    let mut report = |stage| {
        emit(MediaLoad {
            token: read.token.clone(),
            stage,
            bytes_done: read.progress.done(),
            bytes_total: read.range.len(),
        });
    };
    match outcome {
        Ok(body) => {
            report(MediaLoadStage::Done);
            MediaAnswer::Body {
                mime: read.mime,
                range: read.range,
                body,
            }
        }
        Err(MediaReadFailure::Closed) => MediaAnswer::NotFound,
        Err(MediaReadFailure::Stalled) => {
            report(MediaLoadStage::Failed);
            MediaAnswer::TimedOut
        }
        Err(MediaReadFailure::Unreadable) => {
            report(MediaLoadStage::Failed);
            MediaAnswer::NotFound
        }
    }
}

/// One volume token's request in flight: what it reads and how far it got.
pub(super) struct VolumeRead<'a> {
    pub(super) token: String,
    pub(super) mime: String,
    pub(super) range: ResolvedRange,
    pub(super) progress: &'a ReadProgress,
}

/// Serves `range_header` of a volume token's file in one go, for a host with no blocking
/// threads to read on (the browser demo): no stall rule, since nothing can stall an
/// in-memory read. The app's scheme handler reads under a stall rule instead
/// (`media_protocol::serve_volume`), and both finish through [`finish_volume_read`].
pub fn serve_volume_inline(
    media: &VolumeMedia,
    token: &str,
    range_header: Option<&str>,
    emit: impl FnMut(MediaLoad),
) -> MediaAnswer {
    let range = match resolve_range(range_header, media.total_bytes()) {
        RangeOutcome::Unsatisfiable { total } => return MediaAnswer::Unsatisfiable { total },
        RangeOutcome::Serve(range) => range,
    };
    let progress = ReadProgress::new();
    let outcome = media.read(range.start, range.len(), &progress);
    let read = VolumeRead {
        token: token.to_string(),
        mime: media.mime().to_string(),
        range,
        progress: &progress,
    };
    finish_volume_read(outcome, read, emit)
}

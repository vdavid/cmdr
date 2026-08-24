//! PII-free analytics for the file viewer.
//!
//! The viewer is the one place in Cmdr that looks INSIDE a file, so it's also the
//! place where an event is easiest to get wrong. The rule here is absolute: the
//! vocabulary is a three-way content class and a coarse size bucket, and nothing
//! else. ❌ Never a file name, an extension, a path, or a byte count — an
//! extension list would fingerprint a person's work from a population of a few
//! hundred installs, and a raw size does the same for one file.
//!
//! The event fires from `session::open_session_inner`, which is the ONE gate every
//! viewer open passes: F3, the "View as text" override, and preview-inside-a-zip
//! all land there, and its many `?`s mean a per-branch emit would count only the
//! opens that worked.

use super::ViewerError;
use super::content_kind::ViewerContentKind;
use super::session::ViewerOpenResult;

/// The content class as a stable token. Mirrors the enum rather than reusing its
/// serde form, so renaming a variant can't silently rewrite months of history.
pub(super) fn content_token(kind: ViewerContentKind) -> &'static str {
    match kind {
        ViewerContentKind::Text => "text",
        ViewerContentKind::Image => "image",
        ViewerContentKind::Pdf => "pdf",
    }
}

/// A file's size as a bucket. The steps are where the viewer's own behavior
/// changes, so the numbers answer "is the instant-open machinery earning its
/// keep?": under 1 MB is the `FullLoad` path, everything above it opens on
/// `ByteSeek` and upgrades to `LineIndex` in the background, and the top two
/// buckets are where that upgrade takes long enough to notice.
pub(super) fn size_bucket(bytes: u64) -> &'static str {
    const MB: u64 = 1024 * 1024;
    match bytes {
        b if b < MB => "<1MB",
        b if b < 10 * MB => "1-10MB",
        b if b < 100 * MB => "10-100MB",
        b if b < 1024 * MB => "100MB-1GB",
        _ => "1GB+",
    }
}

/// Why an open didn't produce a session, as a token. The variants carry a
/// `message` or a `path`; ❌ none of that crosses — only the variant's name does.
///
/// Four causes of a zero look identical without this: a viewer nobody opens, a
/// viewer people open on files that vanished, an archive preview refused by the
/// zip-bomb cap, and a codec the archive reader can't handle.
pub(super) fn failure_token(error: &ViewerError) -> &'static str {
    match error {
        ViewerError::Io { .. } => "io",
        ViewerError::NotFound { .. } => "not_found",
        ViewerError::IsDirectory => "is_directory",
        ViewerError::SessionNotFound { .. } => "session_not_found",
        ViewerError::Cancelled => "cancelled",
        ViewerError::OutOfRange => "out_of_range",
        ViewerError::TimedOut => "timed_out",
        ViewerError::ExtractTooLarge { .. } => "extract_too_large",
        ViewerError::DestinationInsideArchive => "destination_inside_archive",
        ViewerError::Archive { .. } => "archive",
    }
}

/// Emits `viewer_opened` for one open attempt, successful or not.
///
/// `from_archive` and `forced_text` are the two gestures that would otherwise be
/// invisible: previewing an entry inside a `.zip`, and the toolbar's "View as
/// text" override on a file the classifier called media.
pub(super) fn emit_viewer_opened(
    result: &Result<ViewerOpenResult, ViewerError>,
    from_archive: bool,
    forced_text: bool,
) {
    let (content, size, outcome, failure) = match result {
        Ok(open) => (
            content_token(open.kind),
            size_bucket(open.total_bytes),
            "opened",
            "none",
        ),
        // A failed open has no classified content and no size: the file was never
        // read far enough to have either. `unknown` says so rather than implying
        // an empty text file.
        Err(e) => ("unknown", "unknown", "failed", failure_token(e)),
    };
    crate::analytics::posthog::capture(
        "viewer_opened",
        serde_json::json!({
            "content": content,
            "size_bucket": size,
            "outcome": outcome,
            "failure": failure,
            "from_archive": from_archive,
            "forced_text": forced_text,
        }),
    );
}

//! The media-open path for the file viewer.
//!
//! An `Image` / `Pdf`, on any volume, opens as a *media session*: no text
//! backend, a minted `cmdr-media://` capability token, and best-effort header-only image
//! dimensions. The bytes are served by the `cmdr-media://` scheme handler
//! (the app's `file_viewer/media_protocol.rs`, over [`super::media_serve`]); this module only sets up the session and the token. Text
//! opens (FullLoad / ByteSeek / LineIndex, search, encoding, tail) stay in
//! [`super::session`]; both build on [`super::session_store`] (the `ViewerSession` type and
//! its constructor).
//!
//! [`try_open_media`] is the single entry point [`Viewer::open_session`](super::viewer::Viewer::open_session) calls
//! before it builds a text backend: it classifies the file and, for a media kind, opens
//! the media session and returns the result; otherwise it returns `None` and the caller
//! falls through to the text path.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::content_kind::{CLASSIFY_HEAD_LEN, ViewerContentKind, classify_viewer_content, media_mime};
use super::encoding::FileEncoding;
use super::host::pulls::PreviewTemp;
use super::media::{self, MediaEntry};
use super::media_backend::MediaBackend;
use super::media_volume::{MEDIA_ON_VOLUME_MAX_BYTES, VolumeMedia};
use super::origin::SourceOrigin;
use super::row_text::RowView;
use super::session_store::{
    BackendType, MediaDimensions, ViewerOpenResult, ViewerSession, ViewerSessionInit, generate_session_id,
};
use super::source::{OpenedFile, read_head};
use super::viewer::Viewer;
use super::{FileViewerBackend, LineChunk, ViewerError};

/// One file to classify, through the handle the session opened.
pub(super) struct MediaOpen<'a> {
    /// The path the user asked for: its extension is what SVG's sniff reads.
    pub requested: &'a Path,
    /// Where the session sits: `requested`, or the temp copy of it.
    pub file_path: &'a Path,
    pub origin: &'a SourceOrigin,
    pub opened: &'a OpenedFile,
    pub temp: Option<Arc<PreviewTemp>>,
}

/// Classifies the file by magic bytes, read through its open handle, and, if it's a media
/// kind (`Image` / `Pdf`), opens a media session and returns its result. Returns `None`
/// when the file should flow on through the text pipeline, so the caller can fall through.
pub(super) fn try_open_media(viewer: &Viewer, file: MediaOpen<'_>) -> Option<Result<ViewerOpenResult, ViewerError>> {
    let on_a_volume = matches!(file.origin, SourceOrigin::Volume { .. });
    if on_a_volume && file.opened.total_bytes > MEDIA_ON_VOLUME_MAX_BYTES {
        return None;
    }
    let head = read_head(file.opened.source.as_ref(), file.opened.total_bytes, CLASSIFY_HEAD_LEN).unwrap_or_default();
    let ext = file.requested.extension().and_then(|e| e.to_str());
    let kind = classify_viewer_content(&head, ext);
    matches!(kind, ViewerContentKind::Image | ViewerContentKind::Pdf)
        .then(|| open_media_session(viewer, file, &head, kind))
}

/// Opens a media (Image/PDF) session: no text backend, a minted `cmdr-media://` token,
/// and best-effort header-only image dimensions. Creates a real `ViewerSession` (with a
/// `MediaBackend` no-op) so close/teardown stays uniform with text sessions.
fn open_media_session(
    viewer: &Viewer,
    file: MediaOpen<'_>,
    head: &[u8],
    kind: ViewerContentKind,
) -> Result<ViewerOpenResult, ViewerError> {
    let MediaOpen {
        file_path,
        origin,
        opened,
        temp,
        ..
    } = file;
    let file_name = opened.file_name.clone();
    let file_size = opened.total_bytes;
    let media_from_volume = matches!(origin, SourceOrigin::Volume { .. });

    let mime = media_mime(head, kind).unwrap_or("application/octet-stream").to_string();

    // Mint the capability token. The FE builds the URL from this; the scheme handler
    // resolves it back to a path it opens, or to the session's handle on a volume.
    let media_token = if media_from_volume {
        viewer
            .tokens()
            .mint_volume_token(Arc::new(VolumeMedia::new(Arc::clone(&opened.source), file_size, mime)))
    } else {
        viewer.tokens().mint_token(MediaEntry {
            canonical_path: file_path.to_path_buf(),
            kind,
            mime,
        })
    };

    // Header-only, best-effort image dimensions. Must not extend the open past a quick
    // header read; `read_source_image_dimensions` reads only the header (and returns
    // `None` for HEIC/SVG, which the `image` crate can't parse).
    let media_dimensions = if matches!(kind, ViewerContentKind::Image) {
        media::read_source_image_dimensions(opened.source.as_ref(), file_size)
            .map(|(width, height)| MediaDimensions { width, height })
    } else {
        None
    };

    let backend_box: Box<dyn FileViewerBackend> = Box::new(MediaBackend::new(file_name.clone(), file_size));
    let capabilities = backend_box.capabilities();

    let session_id = generate_session_id();

    // A media session never tail-follows or rebuilds, so its encoding fields are
    // placeholders (`Utf8`) and its watcher-stop flag is inert: no watcher manager is
    // spawned for it.
    let session = ViewerSession::new(ViewerSessionInit {
        backend: backend_box,
        backend_type: BackendType::FullLoad,
        upgrading: None,
        encoding: FileEncoding::Utf8,
        detected_encoding: FileEncoding::Utf8,
        watcher_stop: Arc::new(AtomicBool::new(false)),
        path: file_path.to_path_buf(),
        origin: origin.clone(),
        // A volume's reader stays held for Binary / Hex, which would otherwise reopen it.
        file: media_from_volume.then(|| opened.clone()),
        media_token: Some(media_token.clone()),
        temp,
        markdown: None,
        rows: RowView::raw(),
    });

    // No watcher and no LineIndex upgrade for media: there's no text viewport to
    // tail-follow, and the bytes are re-read fresh per `cmdr-media://` request anyway.
    viewer.sessions().insert(session_id.clone(), session);

    let empty_initial = LineChunk {
        rows: Vec::new(),
        first_row_number: 0,
        byte_offset: 0,
        end_byte_offset: 0,
        end: super::ChunkEnd::EndOfFile,
        total_rows: super::TotalRows::Exact(0),
        total_bytes: file_size,
    };

    Ok(ViewerOpenResult {
        session_id,
        file_name,
        total_bytes: file_size,
        total_lines: Some(0),
        estimated_total_rows: 0,
        backend_type: BackendType::FullLoad,
        capabilities,
        initial_lines: empty_initial,
        is_indexing: false,
        encoding: FileEncoding::Utf8,
        kind,
        media_token: Some(media_token),
        media_dimensions,
        media_from_volume,
    })
}

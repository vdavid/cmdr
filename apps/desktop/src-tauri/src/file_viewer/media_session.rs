//! The media-open path for the file viewer.
//!
//! An `Image` / `Pdf` on a local POSIX volume opens as a *media session*: no text
//! backend, a minted `cmdr-media://` capability token, and best-effort header-only image
//! dimensions. The bytes are served by the `cmdr-media://` scheme handler
//! ([`super::media_protocol`]); this module only sets up the session and the token. Text
//! opens (FullLoad / ByteSeek / LineIndex, search, encoding, tail) stay in
//! [`super::session`]; the two share the `ViewerSession` type and its `SESSIONS` cache.
//!
//! [`try_open_media`] is the single entry point [`super::session::open_session`] calls
//! before it builds a text backend: it classifies the file and, for a media kind, opens
//! the media session and returns the result; otherwise it returns `None` and the caller
//! falls through to the text path.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde::Serialize;

use crate::ignore_poison::IgnorePoison;

use super::content_kind::{CLASSIFY_HEAD_LEN, ViewerContentKind, classify_viewer_content, media_mime};
use super::encoding::FileEncoding;
use super::media::{self, MediaEntry};
use super::media_backend::MediaBackend;
use super::session::{BackendType, SESSIONS, ViewerOpenResult, ViewerSession, ViewerSessionInit, generate_session_id};
use super::{FileViewerBackend, LineChunk, ViewerError};

/// Image pixel dimensions, read header-only at open time.
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaDimensions {
    pub width: u32,
    pub height: u32,
}

/// Classifies `file_path` by magic bytes and, if it's a media kind (`Image` / `Pdf` on a
/// local volume), opens a media session and returns its result. Returns `None` when the
/// file should flow through the text pipeline, so the caller can fall through.
pub(super) fn try_open_media(
    file_path: &Path,
    file_size: u64,
    extract_cleanup: Option<std::path::PathBuf>,
) -> Option<Result<ViewerOpenResult, ViewerError>> {
    let head = read_head(file_path, CLASSIFY_HEAD_LEN);
    let ext = file_path.extension().and_then(|e| e.to_str());
    let is_local = is_local_posix_path(file_path);
    let kind = classify_viewer_content(&head, ext, is_local);
    if matches!(kind, ViewerContentKind::Image | ViewerContentKind::Pdf) {
        Some(open_media_session(file_path, file_size, &head, kind, extract_cleanup))
    } else {
        None
    }
}

/// Opens a media (Image/PDF) session: no text backend, a minted `cmdr-media://` token,
/// and best-effort header-only image dimensions. Creates a real `ViewerSession` (with a
/// `MediaBackend` no-op) so close/teardown stays uniform with text sessions.
fn open_media_session(
    file_path: &Path,
    file_size: u64,
    head: &[u8],
    kind: ViewerContentKind,
    extract_cleanup: Option<std::path::PathBuf>,
) -> Result<ViewerOpenResult, ViewerError> {
    let file_name = file_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let mime = media_mime(head, kind).unwrap_or("application/octet-stream").to_string();

    // Mint the capability token: token -> { canonical_path, kind, mime }. The FE builds
    // the URL from this; the scheme handler resolves it back to the path.
    let media_token = media::mint_token(MediaEntry {
        canonical_path: file_path.to_path_buf(),
        kind,
        mime,
    });

    // Header-only, best-effort image dimensions. Must not extend the open past a quick
    // header read; `image::image_dimensions` reads only the header (and returns `None`
    // for HEIC/SVG, which the `image` crate can't parse).
    let media_dimensions = if matches!(kind, ViewerContentKind::Image) {
        media::read_image_dimensions(file_path).map(|(width, height)| MediaDimensions { width, height })
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
        media_token: Some(media_token.clone()),
        extract_cleanup,
    });

    // No watcher and no LineIndex upgrade for media: there's no text viewport to
    // tail-follow, and the bytes are re-read fresh per `cmdr-media://` request anyway.
    SESSIONS.lock_ignore_poison().insert(session_id.clone(), session);

    let empty_initial = LineChunk {
        lines: Vec::new(),
        first_line_number: 0,
        byte_offset: 0,
        total_lines: Some(0),
        total_bytes: file_size,
    };

    Ok(ViewerOpenResult {
        session_id,
        file_name,
        total_bytes: file_size,
        total_lines: Some(0),
        estimated_total_lines: 0,
        backend_type: BackendType::FullLoad,
        capabilities,
        initial_lines: empty_initial,
        is_indexing: false,
        encoding: FileEncoding::Utf8,
        kind,
        media_token: Some(media_token),
        media_dimensions,
    })
}

/// Reads up to `max` bytes from the start of `file_path` for magic-byte classification.
/// Best-effort: a read error yields an empty slice (-> classified as `Text`).
fn read_head(file_path: &Path, max: usize) -> Vec<u8> {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(file_path) else {
        return Vec::new();
    };
    let mut buf = vec![0u8; max];
    match file.read(&mut buf) {
        Ok(n) => {
            buf.truncate(n);
            buf
        }
        Err(_) => Vec::new(),
    }
}

/// Whether `file_path` lives on a local POSIX volume (the only volumes eligible for
/// media rendering in v1). MTP has no POSIX path; SMB paths can block. We consult the
/// registered volumes: the most-specific volume by mount-root prefix decides. A volume
/// is "local" when it supports `std::fs` access AND is not an SMB mount
/// (`smb_connection_state().is_none()`). When no registered volume claims the path
/// (the path is a real file outside any mount, e.g. under `/`), it's local.
fn is_local_posix_path(file_path: &Path) -> bool {
    let manager = crate::file_system::get_volume_manager();
    let mut best: Option<(usize, bool)> = None; // (root component count, is_local)
    for (_id, volume) in manager.list_volumes_with_handles() {
        let root = volume.root();
        // The "root" LocalPosixVolume roots at "/", a prefix of everything; more
        // specific mounts (longer roots) win the tie.
        if file_path.starts_with(root) {
            let depth = root.components().count();
            let is_local = volume.supports_local_fs_access() && volume.smb_connection_state().is_none();
            if best.is_none_or(|(d, _)| depth > d) {
                best = Some((depth, is_local));
            }
        }
    }
    best.map(|(_, is_local)| is_local).unwrap_or(true)
}

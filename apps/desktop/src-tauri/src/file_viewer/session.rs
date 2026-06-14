//! ViewerSession: orchestrates file viewer backends and manages session lifecycle.
//!
//! Opens a file, picks the right backend based on file size, and provides a session-based
//! API for the frontend. Sessions are cached by ID and cleaned up on close.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, RwLock};
use std::thread;
use std::time::Duration;

use arc_swap::ArcSwap;

use crate::commands::file_system::expand_tilde;
use crate::ignore_poison::IgnorePoison;
use log::debug;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::byte_seek::ByteSeekBackend;
use super::content_kind::{CLASSIFY_HEAD_LEN, ViewerContentKind, classify_viewer_content, media_mime};
use super::encoding::{FileEncoding, detect, same_byte_layout};
use super::full_load::FullLoadBackend;
use super::line_index::LineIndexBackend;
use super::media::{self, MediaEntry};
use super::media_backend::MediaBackend;
use super::range_read::{RangeEnd, read_range as do_read_range};
use super::search_matcher::{Matcher, SearchMode};
use super::watcher::{VIEWER_WATCHER_MANAGER, WatcherEvent};
use super::{
    BackendCapabilities, FULL_LOAD_THRESHOLD, FileViewerBackend, LineChunk, MAX_SEARCH_MATCHES, SearchMatch,
    SeekTarget, ViewerError,
};

/// Process-wide AppHandle for emitting `viewer:file-changed:<sid>` events from
/// background watcher threads. Set during app setup via [`init_app_handle`].
static VIEWER_APP_HANDLE: LazyLock<RwLock<Option<AppHandle>>> = LazyLock::new(|| RwLock::new(None));

/// Stash the AppHandle so the per-session watcher manager threads can emit
/// `viewer:file-changed:<session_id>` events.
pub fn init_app_handle(handle: AppHandle) {
    if let Ok(mut guard) = VIEWER_APP_HANDLE.write() {
        *guard = Some(handle);
    }
}

fn app_handle() -> Option<AppHandle> {
    VIEWER_APP_HANDLE.read().ok().and_then(|g| g.clone())
}

/// Which backend strategy is active for a session.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum BackendType {
    FullLoad,
    ByteSeek,
    LineIndex,
}

/// One row in the encoding dropdown.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EncodingChoice {
    pub encoding: FileEncoding,
    pub label: String,
    pub group: super::encoding::EncodingGroup,
}

/// Returned by `viewer_get_encoding_options`: current selection, detected encoding, and
/// the full list of dropdown rows. The FE shows `detected` with a "(Detected)" suffix.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EncodingOptions {
    pub current: FileEncoding,
    pub detected: FileEncoding,
    pub all: Vec<EncodingChoice>,
}

/// All encodings the viewer offers, in dropdown order (Unicode first, then Western).
fn all_encoding_choices() -> Vec<EncodingChoice> {
    use FileEncoding::*;
    [
        Utf8,
        Utf8WithBom,
        Utf16Le,
        Utf16Be,
        Windows1252,
        Iso8859_1,
        MacRoman,
        UsAscii,
    ]
    .iter()
    .map(|enc| EncodingChoice {
        encoding: *enc,
        label: enc.label().to_string(),
        group: enc.group(),
    })
    .collect()
}

/// Result returned when opening a viewer session.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ViewerOpenResult {
    pub session_id: String,
    pub file_name: String,
    pub total_bytes: u64,
    pub total_lines: Option<usize>,
    /// For ByteSeek where `total_lines` is unknown. Based on `total_bytes / avg_bytes_per_line`.
    pub estimated_total_lines: usize,
    pub backend_type: BackendType,
    pub capabilities: BackendCapabilities,
    pub initial_lines: LineChunk,
    /// ByteSeek -> LineIndex upgrade in progress.
    pub is_indexing: bool,
    /// Auto-detected encoding (also the initial selection of the picker).
    pub encoding: FileEncoding,
    /// Detected content kind. `Text` flows through the line pipeline (the fields
    /// above are populated); `Image` / `Pdf` render inline from `media_token` and
    /// leave the text fields empty.
    pub kind: ViewerContentKind,
    /// Present only for media kinds (`Image` / `Pdf`): the unguessable token the FE
    /// puts in the `cmdr-media://localhost/<token>` URL. `None` for text.
    pub media_token: Option<String>,
    /// Image pixel dimensions, header-only and best-effort. `Some` only for some
    /// `Image` files (raster formats the `image` crate can parse; `None` for HEIC,
    /// SVG, PDFs, text, or on any read error).
    pub media_dimensions: Option<MediaDimensions>,
}

/// Image pixel dimensions, read header-only at open time.
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaDimensions {
    pub width: u32,
    pub height: u32,
}

/// Current status of a viewer session.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ViewerSessionStatus {
    pub backend_type: BackendType,
    pub is_indexing: bool,
    pub total_lines: Option<usize>,
}

/// Status of an ongoing search.
///
/// `InvalidQuery` carries the user-facing reason (invalid regex syntax, multiline
/// pattern, regex exceeds size limits). Surfaced via `search_poll`; the FE renders
/// the message as plain text without inspecting its contents (per the
/// no-error-string-match rule).
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "status", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SearchStatus {
    Running,
    Done,
    Cancelled,
    Idle,
    InvalidQuery { message: String },
}

/// Result from polling search progress.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchPollResult {
    pub status: SearchStatus,
    /// Only matches discovered since the caller's `since_index`. The caller accumulates
    /// these locally, so each poll transfers only the delta.
    pub new_matches: Vec<SearchMatch>,
    /// Authoritative total match count (including matches the caller already has).
    pub total_match_count: usize,
    pub total_bytes: u64,
    pub bytes_scanned: u64,
    /// True when the match list was capped at MAX_SEARCH_MATCHES. The search kept scanning
    /// (for progress) but stopped storing new matches.
    pub match_limit_reached: bool,
}

/// Internal state for an active search.
struct SearchState {
    cancel: Arc<AtomicBool>,
    matches: Arc<Mutex<Vec<SearchMatch>>>,
    bytes_scanned: Arc<Mutex<u64>>,
    status: Arc<Mutex<SearchStatus>>,
}

/// A viewer session wraps a backend and tracks search state.
struct ViewerSession {
    /// `ArcSwap` so background threads (ByteSeek → LineIndex upgrade, encoding rebuild,
    /// tail-mode `extend_to`) can replace the backend without a write lock on the
    /// `get_lines` read path. Each backend is immutable; readers pick up either the
    /// old or new backend atomically. Document rationale: `file_viewer/CLAUDE.md`
    /// § "ArcSwap rather than RwLock."
    backend: Arc<ArcSwap<Box<dyn FileViewerBackend>>>,
    backend_type: Mutex<BackendType>,
    search: Option<SearchState>,
    /// Set when upgrading from ByteSeek to LineIndex in the background.
    upgrading: Mutex<Option<Arc<AtomicBool>>>,
    /// Set when an encoding-switch rebuild is in flight. Same `AtomicBool` pattern as
    /// `upgrading`: the rebuild thread reads it to see if it's been superseded by a
    /// rapid follow-up `set_encoding`.
    rebuilding: Mutex<Option<Arc<AtomicBool>>>,
    /// Latest pending `Grew(eof)` from the (future) watcher manager. Both the upgrade
    /// thread and the encoding-rebuild thread drain this inside their swap critical
    /// section so a tail append arriving mid-rebuild isn't silently dropped. Documented
    /// protocol: drain-read → optional `extend_to` → `ArcSwap::store` → clear flag,
    /// all under one mutex lock.
    pending_grew: Mutex<Option<u64>>,
    /// Current encoding. Updated atomically with the backend swap on `set_encoding`.
    encoding: Mutex<FileEncoding>,
    /// Detected encoding at open time (sticky; never changes after `open_session`).
    detected_encoding: FileEncoding,
    /// Tail mode flag: when true, `Grew` watcher events trigger a backend
    /// `extend_to` so the open viewport auto-follows newly appended bytes.
    /// When false, the FE still hears `viewer:file-changed:<sid>` events and
    /// renders the persistent reload toast.
    tail_mode: AtomicBool,
    /// Cancel flag the manager thread reads on session close. Dropping the
    /// session sets this flag; the manager thread observes it on its next
    /// receive cycle, drops its owned `ViewerSubscription`, and exits. The
    /// subscription's `Drop` then unwatches the path via the shared singleton.
    watcher_stop: Arc<AtomicBool>,
    /// Per-read cancel flags. Each `read_range` call inserts an entry keyed by the FE's
    /// `read_id`, removes it on completion (cancelled or not). The FE generates fresh
    /// ids per call (a monotonic counter is fine; uniqueness within a session is all
    /// that's needed). Per-read (not session-wide) so a cancel for one read doesn't
    /// affect a follow-up read that started in the same gesture.
    active_reads: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    path: PathBuf,
    /// The `cmdr-media://` token for a media (Image/PDF) session, dropped from the
    /// global token map at `close_session`. `None` for text sessions. Tying the drop
    /// to the single close choke point keeps the token's lifetime exactly the
    /// session's, so a closed-window viewer can't leave a live token mapping a path.
    media_token: Option<String>,
}

impl ViewerSession {
    /// Loads a freshly cloned `Arc` to the current backend. Holds no lock so calls to
    /// `get_lines` can run in parallel.
    fn load_backend(&self) -> Arc<Box<dyn FileViewerBackend>> {
        self.backend.load_full()
    }
}

/// Global session cache.
static SESSIONS: LazyLock<Mutex<HashMap<String, ViewerSession>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Maps a viewer window label (`viewer-<timestamp>`) to its session id.
///
/// The FE creates the window and then opens the session, so the only link
/// between a window and its session is the FE. We record it here at
/// `open_session` time so the Rust-side window-destroyed handler can free the
/// session when the user closes the window via the titlebar X — a path that
/// never fires the FE `viewer_close` IPC. Without this, those sessions (and
/// their backends, watcher threads, line indexes) leaked until app quit.
static WINDOW_TO_SESSION: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Records the window-label → session-id link. Called from `viewer_open` with
/// the opening window's label. A label with no session (empty string) is ignored.
pub fn register_window_session(window_label: &str, session_id: &str) {
    if window_label.is_empty() {
        return;
    }
    WINDOW_TO_SESSION
        .lock_ignore_poison()
        .insert(window_label.to_string(), session_id.to_string());
}

/// Frees the session owned by `window_label`, if any. Called from the window
/// `Destroyed`/`CloseRequested` handler for `viewer-*` windows. Idempotent: a
/// window with no recorded session (or an already-closed session) is a no-op.
pub fn close_session_for_window(window_label: &str) {
    let session_id = WINDOW_TO_SESSION.lock_ignore_poison().remove(window_label);
    if let Some(session_id) = session_id {
        // Reuse the normal teardown; ignore SessionNotFound (the FE may have
        // already closed it via `viewer_close`).
        let _ = close_session(&session_id);
    }
}

/// Number of initial lines to return on open.
const INITIAL_LINE_COUNT: usize = 200;

/// Maximum time to spend building the line index before giving up.
/// If indexing takes longer, the session stays in ByteSeek (streaming) mode.
/// This prevents hammering slow disks or network drives.
const INDEXING_TIMEOUT_SECS: u64 = 5;

/// Generates a unique session ID.
fn generate_session_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Opens a viewer session for the given file path.
///
/// Classifies the file by magic bytes first: an `Image` / `Pdf` on a local volume
/// opens as a media session (no text backend; the bytes are served via the
/// `cmdr-media://` scheme), everything else as a text session that picks a backend by
/// file size:
/// - Under 1 MB: FullLoad (instant, full random access)
/// - Over 1 MB: ByteSeek first (instant open), then upgrades to LineIndex in background
pub fn open_session(path: &str) -> Result<ViewerOpenResult, ViewerError> {
    open_session_inner(path, /*force_text=*/ false)
}

/// Opens a fresh, full text session regardless of content kind. Backs the "View as
/// text" override: a media session isn't upgraded in place; the FE swaps to the
/// session this returns. Reuses the text path verbatim.
pub fn open_session_as_text(path: &str) -> Result<ViewerOpenResult, ViewerError> {
    open_session_inner(path, /*force_text=*/ true)
}

fn open_session_inner(path: &str, force_text: bool) -> Result<ViewerOpenResult, ViewerError> {
    let expanded = expand_tilde(path);
    let file_path = PathBuf::from(&expanded);

    if !file_path.exists() {
        return Err(ViewerError::NotFound { path: path.to_string() });
    }
    if file_path.is_dir() {
        return Err(ViewerError::IsDirectory);
    }

    let metadata = std::fs::metadata(&file_path)?;
    let file_size = metadata.len();

    // Classify by magic bytes (unless the caller forced text, e.g. "View as text").
    // Media kinds open a no-op session that serves bytes via `cmdr-media://`.
    if !force_text {
        let head = read_head(&file_path, CLASSIFY_HEAD_LEN);
        let ext = file_path.extension().and_then(|e| e.to_str());
        let is_local = is_local_posix_path(&file_path);
        let kind = classify_viewer_content(&head, ext, is_local);
        if matches!(kind, ViewerContentKind::Image | ViewerContentKind::Pdf) {
            return open_media_session(&file_path, file_size, &head, kind);
        }
    }

    // Auto-detect encoding at open time. Used as the initial encoding for every backend.
    let detected_encoding = detect(&file_path).unwrap_or(FileEncoding::Utf8);

    let (backend_box, backend_type, upgrading): (Box<dyn FileViewerBackend>, BackendType, Option<Arc<AtomicBool>>) =
        if file_size <= FULL_LOAD_THRESHOLD {
            let b = FullLoadBackend::open_with_encoding(&file_path, detected_encoding)?;
            (Box::new(b), BackendType::FullLoad, None)
        } else {
            // Start with ByteSeek (instant), then upgrade to LineIndex in background
            let b = ByteSeekBackend::open_with_encoding(&file_path, detected_encoding)?;
            let cancel = Arc::new(AtomicBool::new(false));
            (Box::new(b), BackendType::ByteSeek, Some(cancel))
        };

    // Get initial lines
    let initial_lines = backend_box.get_lines(&SeekTarget::Line(0), INITIAL_LINE_COUNT)?;
    let capabilities = backend_box.capabilities();
    let total_bytes = backend_box.total_bytes();
    let total_lines = backend_box.total_lines();
    let file_name = backend_box.file_name().to_string();

    let backend: Arc<ArcSwap<Box<dyn FileViewerBackend>>> = Arc::new(ArcSwap::from_pointee(backend_box));

    let session_id = generate_session_id();
    let upgrade_cancel = upgrading.clone();
    let is_indexing = upgrade_cancel.is_some();

    let watcher_stop = Arc::new(AtomicBool::new(false));
    let watcher_stop_for_thread = watcher_stop.clone();

    let session = ViewerSession {
        backend,
        backend_type: Mutex::new(backend_type.clone()),
        search: None,
        upgrading: Mutex::new(upgrade_cancel.clone()),
        rebuilding: Mutex::new(None),
        pending_grew: Mutex::new(None),
        encoding: Mutex::new(detected_encoding),
        detected_encoding,
        tail_mode: AtomicBool::new(false),
        watcher_stop,
        active_reads: Mutex::new(HashMap::new()),
        path: file_path.clone(),
        media_token: None,
    };

    // Calculate estimated total lines from the initial sample
    let estimated_total_lines = if let Some(lines) = total_lines {
        // If we know the exact count, use it
        lines
    } else if !initial_lines.lines.is_empty() {
        // Estimate from initial sample: total_bytes / avg_bytes_per_line
        let total_bytes_in_sample: usize = initial_lines.lines.iter().map(|l| l.len() + 1).sum(); // +1 for newline
        let avg_bytes_per_line = total_bytes_in_sample / initial_lines.lines.len();
        (total_bytes as usize)
            .checked_div(avg_bytes_per_line)
            .unwrap_or((total_bytes as usize) / 80) // fallback when avg is 0
    } else {
        (total_bytes as usize) / 80 // fallback for empty files
    };

    let result = ViewerOpenResult {
        session_id: session_id.clone(),
        file_name,
        total_bytes,
        total_lines,
        estimated_total_lines,
        backend_type,
        capabilities,
        initial_lines,
        is_indexing,
        encoding: detected_encoding,
        kind: ViewerContentKind::Text,
        media_token: None,
        media_dimensions: None,
    };

    let session_path = session.path.clone();
    SESSIONS.lock_ignore_poison().insert(session_id.clone(), session);

    // Attach the filesystem watcher off the critical path. The FSEvents
    // subscribe is a blocking, `fseventsd`-bound call (~100ms idle, seconds
    // under load), so doing it inline would make every viewer open pay that
    // latency and risk the 2s `viewer_open` timeout on a busy system. The
    // manager thread does the subscribe itself, then a catch-up re-stat closes
    // the open→subscribe window for any append that landed before the watcher
    // went live. See `spawn_watcher_manager`.
    //
    // Tests can opt out via `CMDR_VIEWER_DISABLE_WATCHER=1`.
    if std::env::var("CMDR_VIEWER_DISABLE_WATCHER").is_err() {
        spawn_watcher_manager(session_id.clone(), session_path, watcher_stop_for_thread);
    }

    // If we're using ByteSeek, start background upgrade to LineIndex with timeout
    if let Some(cancel_flag) = upgrade_cancel {
        let session_id_clone = session_id.clone();
        let path_clone = file_path.clone();
        let cancel_for_indexer = cancel_flag.clone();
        let cancel_for_timeout = cancel_flag.clone();

        // Spawn timeout thread that cancels indexing after INDEXING_TIMEOUT_SECS
        let session_id_for_timeout = session_id.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(INDEXING_TIMEOUT_SECS));
            // If still indexing (flag not already set), cancel it
            if !cancel_for_timeout.load(Ordering::Relaxed) {
                debug!(
                    "Indexing timeout reached for session {}, cancelling",
                    session_id_for_timeout
                );
                cancel_for_timeout.store(true, Ordering::Relaxed);
                // Mark session as no longer indexing
                if let Ok(sessions) = SESSIONS.lock()
                    && let Some(session) = sessions.get(&session_id_for_timeout)
                {
                    *session.upgrading.lock_ignore_poison() = None;
                    // Drop any pending tail-append: ByteSeek has no line index to
                    // extend, so a queued EOF is no-op until next FS event.
                    *session.pending_grew.lock_ignore_poison() = None;
                }
            }
        });

        // Spawn indexing thread.
        // The upgrade follows the drain-and-swap-under-lock protocol from
        // `file_viewer/CLAUDE.md` § "Drain-and-swap protocol": after scanning, we
        // acquire `pending_grew` lock and (a) optionally extend the new LineIndex to
        // the latest tail-mode EOF, (b) `ArcSwap::store` the new backend, (c) clear
        // `upgrading`, all inside one critical section. This makes the swap atomic
        // from the watcher's point of view; any append that arrives during the
        // window is queued in `pending_grew` and consumed by the swap.
        let encoding_for_upgrade = detected_encoding;
        thread::spawn(move || {
            // Test-only sleep hook: lets tests pin the upgrade thread mid-scan
            // so they can race appends or other operations against the drain.
            #[cfg(test)]
            {
                // `swap` self-clears so the next upgrade in this test process
                // doesn't accidentally inherit the hold.
                let hold = UPGRADE_SLEEP_HOOK.swap(0, Ordering::SeqCst);
                if hold > 0 {
                    thread::sleep(Duration::from_millis(hold));
                }
            }
            match LineIndexBackend::open_with_encoding(&path_clone, encoding_for_upgrade, &cancel_for_indexer) {
                Ok(new_backend) => {
                    if !cancel_for_indexer.load(Ordering::Relaxed) {
                        let sessions = SESSIONS.lock_ignore_poison();
                        if let Some(session) = sessions.get(&session_id_clone) {
                            debug!(
                                "Indexing completed for session {}, upgrading to LineIndex",
                                session_id_clone
                            );
                            // Drain-and-swap critical section.
                            let mut pending_lock = session.pending_grew.lock_ignore_poison();
                            let pending = pending_lock.take();
                            let final_backend: Box<dyn FileViewerBackend> = match pending {
                                Some(eof) if eof > new_backend.total_bytes() => {
                                    match new_backend.extend_to(eof, &cancel_for_indexer) {
                                        Ok(extended) => Box::new(extended),
                                        Err(_) => Box::new(new_backend),
                                    }
                                }
                                _ => Box::new(new_backend),
                            };
                            session.backend.store(Arc::new(final_backend));
                            *session.backend_type.lock_ignore_poison() = BackendType::LineIndex;
                            *session.upgrading.lock_ignore_poison() = None;
                            drop(pending_lock);
                        }
                    }
                }
                Err(e) => {
                    debug!("Indexing failed for session {}: {}", session_id_clone, e);
                    if let Ok(sessions) = SESSIONS.lock()
                        && let Some(session) = sessions.get(&session_id_clone)
                    {
                        *session.upgrading.lock_ignore_poison() = None;
                    }
                }
            }
        });
    }

    Ok(result)
}

/// Opens a media (Image/PDF) session: no text backend, a minted `cmdr-media://` token,
/// and best-effort header-only image dimensions. Creates a real `ViewerSession` (with a
/// `MediaBackend` no-op) so close/teardown stays uniform with text sessions.
fn open_media_session(
    file_path: &Path,
    file_size: u64,
    head: &[u8],
    kind: ViewerContentKind,
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
    let backend: Arc<ArcSwap<Box<dyn FileViewerBackend>>> = Arc::new(ArcSwap::from_pointee(backend_box));

    let session_id = generate_session_id();
    let watcher_stop = Arc::new(AtomicBool::new(false));

    let session = ViewerSession {
        backend,
        backend_type: Mutex::new(BackendType::FullLoad),
        search: None,
        upgrading: Mutex::new(None),
        rebuilding: Mutex::new(None),
        pending_grew: Mutex::new(None),
        encoding: Mutex::new(FileEncoding::Utf8),
        detected_encoding: FileEncoding::Utf8,
        tail_mode: AtomicBool::new(false),
        watcher_stop,
        active_reads: Mutex::new(HashMap::new()),
        path: file_path.to_path_buf(),
        media_token: Some(media_token.clone()),
    };

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

/// Test-only millisecond sleep injected at the start of the upgrade thread so
/// tests can race appends or other operations against the drain-and-swap
/// critical section deterministically. `0` disables the hold.
#[cfg(test)]
static UPGRADE_SLEEP_HOOK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Test-only: park the next upgrade thread for `ms` milliseconds before it
/// starts scanning. Resets back to `0` after the spawn observes it.
#[cfg(test)]
#[allow(dead_code, reason = "consumed by session_test race-coverage tests")]
pub fn test_only_set_upgrade_hold(ms: u64) {
    UPGRADE_SLEEP_HOOK.store(ms, Ordering::SeqCst);
}

/// Test-only: park the next encoding-rebuild thread for `ms` milliseconds.
/// Self-clears after the spawn observes it.
#[cfg(test)]
static REBUILD_SLEEP_HOOK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(test)]
#[allow(dead_code, reason = "consumed by session_test rebuild-serialization test")]
pub fn test_only_set_rebuild_hold(ms: u64) {
    REBUILD_SLEEP_HOOK.store(ms, Ordering::SeqCst);
}

/// Gets the current status of a session (backend type, indexing state).
pub fn get_session_status(session_id: &str) -> Result<ViewerSessionStatus, ViewerError> {
    let sessions = SESSIONS.lock_ignore_poison();
    let session = sessions.get(session_id).ok_or_else(|| ViewerError::SessionNotFound {
        session_id: session_id.to_string(),
    })?;
    let backend = session.load_backend();
    let is_indexing =
        session.upgrading.lock_ignore_poison().is_some() || session.rebuilding.lock_ignore_poison().is_some();

    Ok(ViewerSessionStatus {
        backend_type: session.backend_type.lock_ignore_poison().clone(),
        is_indexing,
        total_lines: backend.total_lines(),
    })
}

/// Gets a range of lines from a session.
pub fn get_lines(session_id: &str, target: SeekTarget, count: usize) -> Result<LineChunk, ViewerError> {
    let (backend, backend_type) = {
        let sessions = SESSIONS.lock_ignore_poison();
        let session = sessions.get(session_id).ok_or_else(|| ViewerError::SessionNotFound {
            session_id: session_id.to_string(),
        })?;
        (
            session.load_backend(),
            session.backend_type.lock_ignore_poison().clone(),
        )
    };

    debug!(
        "get_lines: session={}, backend_type={:?}, target={:?}, count={}",
        session_id, backend_type, target, count
    );

    backend.get_lines(&target, count)
}

/// Starts a background search in the given session.
/// Any previous search is cancelled first.
///
/// Builds a `Matcher` from `query` + `mode`. On `MatcherBuildError` the session's
/// search status is set to `InvalidQuery { message }` synchronously (no worker is
/// spawned); the FE picks this up via `search_poll`. Returning `Ok(())` mirrors
/// the existing IPC shape: callers don't need to disambiguate "started a worker"
/// from "marked the query as invalid" because both lead to the same `search_poll`
/// flow.
pub fn search_start(session_id: &str, query: String, mode: SearchMode) -> Result<(), ViewerError> {
    // First, cancel any existing search
    search_cancel(session_id)?;

    let cancel = Arc::new(AtomicBool::new(false));
    let matches: Arc<Mutex<Vec<SearchMatch>>> = Arc::new(Mutex::new(Vec::new()));
    let bytes_scanned: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));

    // Build the matcher up front: an invalid query short-circuits without spawning
    // a worker thread.
    let matcher = match Matcher::build(&query, mode) {
        Ok(m) => m,
        Err(err) => {
            // `MatcherBuildError`'s Display impl owns the user-facing copy; we just
            // forward it through. Keeps the wording in one place.
            let message = err.to_string();
            let status: Arc<Mutex<SearchStatus>> = Arc::new(Mutex::new(SearchStatus::InvalidQuery { message }));
            let search_state = SearchState {
                cancel: cancel.clone(),
                matches: matches.clone(),
                bytes_scanned: bytes_scanned.clone(),
                status,
            };
            let mut sessions = SESSIONS.lock_ignore_poison();
            let session = sessions
                .get_mut(session_id)
                .ok_or_else(|| ViewerError::SessionNotFound {
                    session_id: session_id.to_string(),
                })?;
            session.search = Some(search_state);
            return Ok(());
        }
    };

    let status: Arc<Mutex<SearchStatus>> = Arc::new(Mutex::new(SearchStatus::Running));

    let search_state = SearchState {
        cancel: cancel.clone(),
        matches: matches.clone(),
        bytes_scanned: bytes_scanned.clone(),
        status: status.clone(),
    };

    // Get the file path from the session to open a fresh file handle in the search thread
    let path = {
        let mut sessions = SESSIONS.lock_ignore_poison();
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| ViewerError::SessionNotFound {
                session_id: session_id.to_string(),
            })?;
        session.search = Some(search_state);
        session.path.clone()
    };

    spawn_search_worker(path, matcher, cancel, matches, bytes_scanned, status);
    Ok(())
}

/// Spawns the worker thread that drives a single search. Factored out for clarity;
/// the worker's final status write must be under the same mutex critical section as
/// the watchdog's so a watchdog-set `Cancelled` is sticky (see step 1.4 of the
/// viewer-search plan).
fn spawn_search_worker(
    path: PathBuf,
    matcher: Matcher,
    cancel: Arc<AtomicBool>,
    matches: Arc<Mutex<Vec<SearchMatch>>>,
    bytes_scanned: Arc<Mutex<u64>>,
    status: Arc<Mutex<SearchStatus>>,
) {
    // The watchdog observes the worker via the shared `cancel` flag + `status`
    // mutex; we spawn it inside the worker thread so closing the session before
    // the worker starts doesn't leak a watchdog.
    let watchdog_cancel = cancel.clone();
    let watchdog_status = status.clone();

    thread::spawn(move || {
        let watchdog_handle = thread::spawn(move || run_search_watchdog(watchdog_cancel, watchdog_status));

        // Use ByteSeekBackend for streaming search (low memory, works on any file)
        let backend = match ByteSeekBackend::open(&path) {
            Ok(b) => b,
            Err(_) => {
                finalize_search_status(&status, &cancel, /*errored=*/ true);
                let _ = watchdog_handle.join();
                return;
            }
        };

        let result = backend.search(&matcher, &cancel, &matches, &bytes_scanned);

        finalize_search_status(&status, &cancel, /*errored=*/ result.is_err());
        // Joining the watchdog is best-effort; it exits as soon as it sees a
        // non-Running status (which finalize_search_status just wrote).
        let _ = watchdog_handle.join();
    });
}

/// Writes the worker's final status under the same lock the watchdog uses, so
/// `SearchStatus::Cancelled` is sticky. If the watchdog already wrote
/// `Cancelled`, this is a no-op; otherwise we promote `Running` to one of
/// `Cancelled`, `Done`, or leave-in-place when an error path already wrote.
pub(super) fn finalize_search_status(status: &Arc<Mutex<SearchStatus>>, cancel: &Arc<AtomicBool>, errored: bool) {
    let mut guard = status.lock_ignore_poison();
    *guard = match &*guard {
        // Watchdog won the race: keep the Cancelled verdict.
        SearchStatus::Cancelled => SearchStatus::Cancelled,
        // Invalid-query write happens on the caller's thread before the worker
        // is even spawned; the worker never runs in that case. Defensive in
        // case future refactors interleave them.
        SearchStatus::InvalidQuery { .. } => return,
        // Normal completion: pick Cancelled (the cooperative path) or Done.
        _ => {
            if cancel.load(Ordering::Relaxed) {
                SearchStatus::Cancelled
            } else if errored {
                // Failures (file vanished mid-scan, IO error) map to Done with no
                // matches: the user already saw whatever the worker reported, and
                // a transient IO error isn't worth its own surface today.
                SearchStatus::Done
            } else {
                SearchStatus::Done
            }
        }
    };
}

/// Watchdog: polls the worker's `cancel` flag and forces the search status to
/// `Cancelled` if the worker hasn't observed the flag within 1 s. Exits as soon
/// as the worker writes a non-Running status (i.e. it finished naturally or got
/// cancelled cooperatively).
///
/// The 250 ms poll + 1 s budget pair keeps user-visible cancellation under
/// 1.25 s in the worst case even for runaway-regex paths where the inner
/// `iter.next()` call doesn't observe the per-match cancel.
pub(super) fn run_search_watchdog(cancel: Arc<AtomicBool>, status: Arc<Mutex<SearchStatus>>) {
    let mut cancel_seen_at: Option<std::time::Instant> = None;
    loop {
        thread::sleep(Duration::from_millis(250));
        // Cheap check first; bail out if the worker is done.
        let still_running = matches!(*status.lock_ignore_poison(), SearchStatus::Running);
        if !still_running {
            return;
        }
        if cancel.load(Ordering::Relaxed) {
            let started = cancel_seen_at.get_or_insert_with(std::time::Instant::now);
            if started.elapsed() >= Duration::from_secs(1) {
                let mut guard = status.lock_ignore_poison();
                if matches!(*guard, SearchStatus::Running) {
                    *guard = SearchStatus::Cancelled;
                }
                return;
            }
        }
    }
}

/// Polls search progress for a session.
///
/// `since_index` is the number of matches the caller already has. Only matches after
/// that index are returned, so each poll transfers only the delta.
pub fn search_poll(session_id: &str, since_index: usize) -> Result<SearchPollResult, ViewerError> {
    let sessions = SESSIONS.lock_ignore_poison();
    let session = sessions.get(session_id).ok_or_else(|| ViewerError::SessionNotFound {
        session_id: session_id.to_string(),
    })?;

    let total_bytes = session.load_backend().total_bytes();

    match &session.search {
        None => Ok(SearchPollResult {
            status: SearchStatus::Idle,
            new_matches: Vec::new(),
            total_match_count: 0,
            total_bytes,
            bytes_scanned: 0,
            match_limit_reached: false,
        }),
        Some(search) => {
            let status = search.status.lock_ignore_poison().clone();
            let matches = search.matches.lock_ignore_poison();
            let total_match_count = matches.len();
            let new_matches = if since_index < matches.len() {
                matches[since_index..].to_vec()
            } else {
                Vec::new()
            };
            drop(matches);
            let bytes_scanned = *search.bytes_scanned.lock_ignore_poison();
            let match_limit_reached = total_match_count >= MAX_SEARCH_MATCHES;

            Ok(SearchPollResult {
                status,
                new_matches,
                total_match_count,
                total_bytes,
                bytes_scanned,
                match_limit_reached,
            })
        }
    }
}

/// Cancels an ongoing search.
///
/// Sets the cancel flag and leaves the `SearchState` in place so the spawned
/// search thread can write the final `SearchStatus::Cancelled` and so
/// subsequent `search_poll` calls surface that transition to the FE. The
/// `SearchState` is replaced atomically the next time `search_start` is called
/// (which itself begins by calling this function, then installs a fresh state).
pub fn search_cancel(session_id: &str) -> Result<(), ViewerError> {
    let mut sessions = SESSIONS.lock_ignore_poison();
    let session = sessions
        .get_mut(session_id)
        .ok_or_else(|| ViewerError::SessionNotFound {
            session_id: session_id.to_string(),
        })?;

    if let Some(search) = &session.search {
        search.cancel.store(true, Ordering::Relaxed);
    }

    Ok(())
}

/// Reads a logical range of the file as a single UTF-8 string.
///
/// `read_id` is caller-provided so the FE can call `cancel_read(session_id, read_id)`
/// from a separate gesture (Escape) without an extra round-trip to learn the id. The id
/// only has to be unique within the session's active reads; the FE uses a monotonic
/// counter.
///
/// Holds the global SESSIONS lock only long enough to clone the backend `Arc` and to
/// register the cancel flag. The actual read iterates lines outside the lock, so other
/// commands (`cancel_read`, `get_session_status`, line fetches) stay responsive while a
/// large copy is in flight.
pub fn read_range(session_id: &str, read_id: u64, anchor: RangeEnd, focus: RangeEnd) -> Result<String, ViewerError> {
    let (backend, cancel_flag) = {
        let sessions = SESSIONS.lock_ignore_poison();
        let session = sessions.get(session_id).ok_or_else(|| ViewerError::SessionNotFound {
            session_id: session_id.to_string(),
        })?;
        let flag = Arc::new(AtomicBool::new(false));
        session.active_reads.lock_ignore_poison().insert(read_id, flag.clone());
        (session.load_backend(), flag)
    };

    let result = do_read_range(backend.as_ref().as_ref(), anchor, focus, &cancel_flag);

    // Always unregister, whether the read succeeded, failed, or was cancelled.
    if let Ok(sessions) = SESSIONS.lock()
        && let Some(session) = sessions.get(session_id)
    {
        session.active_reads.lock_ignore_poison().remove(&read_id);
    }

    result
}

/// Reads a range and writes it atomically to `dest_path`. Uses the same `read_id`
/// cancellation plumbing as `read_range`. Write is temp+rename for crash-safety: if
/// the process dies mid-write, the user keeps their original file (if any) instead of
/// a half-written one.
///
/// On success, returns `Ok(())`. On `Cancelled`, the temp file is cleaned up. On
/// any other error, the temp file is best-effort cleaned up and the error is returned
/// typed.
pub fn write_range_to_file(
    session_id: &str,
    read_id: u64,
    anchor: RangeEnd,
    focus: RangeEnd,
    dest_path: &Path,
) -> Result<(), ViewerError> {
    let text = read_range(session_id, read_id, anchor, focus)?;

    // Atomic write: write to `<dest>.cmdr-tmp.<read_id>`, then rename. The same-FS
    // rename gives us atomicity on local volumes (and is best-effort elsewhere).
    let tmp_path = dest_path.with_extension(format!(
        "{}cmdr-tmp.{}",
        dest_path
            .extension()
            .map(|e| format!("{}.", e.to_string_lossy()))
            .unwrap_or_default(),
        read_id
    ));

    std::fs::write(&tmp_path, &text)?;
    if let Err(e) = std::fs::rename(&tmp_path, dest_path) {
        // Best-effort cleanup; ignore secondary errors.
        let _ = std::fs::remove_file(&tmp_path);
        return Err(ViewerError::Io { message: e.to_string() });
    }
    Ok(())
}

/// Flips the cancel flag for an in-flight read. No-op if the read has already finished
/// (the entry was removed from `active_reads` when the read returned).
pub fn cancel_read(session_id: &str, read_id: u64) -> Result<(), ViewerError> {
    let sessions = SESSIONS.lock_ignore_poison();
    let session = sessions.get(session_id).ok_or_else(|| ViewerError::SessionNotFound {
        session_id: session_id.to_string(),
    })?;
    if let Some(flag) = session.active_reads.lock_ignore_poison().get(&read_id) {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

/// Returns the list of selectable encodings plus the currently-active and detected
/// encoding for this session. The FE renders the dropdown directly from this; no
/// hard-coded encoding list lives on the FE.
pub fn get_encoding_options(session_id: &str) -> Result<EncodingOptions, ViewerError> {
    let sessions = SESSIONS.lock_ignore_poison();
    let session = sessions.get(session_id).ok_or_else(|| ViewerError::SessionNotFound {
        session_id: session_id.to_string(),
    })?;
    Ok(EncodingOptions {
        current: *session.encoding.lock_ignore_poison(),
        detected: session.detected_encoding,
        all: all_encoding_choices(),
    })
}

/// Switches the active encoding for an open session.
///
/// Two modes:
///
/// - **Instant** when [`same_byte_layout`] holds: only the decoder changes; the
///   newline index stays valid. This is the UTF-8 ↔ Windows-1252 ↔ Mac Roman case.
///   The current backend's `encoding` field is read by `get_lines` / `search`, so we
///   need to actually swap in a fresh backend with the new encoding — but no
///   reindex is needed. ByteSeek and FullLoad just reopen with the new encoding;
///   for LineIndex we'd want to keep the index. We approximate this today by
///   reopening LineIndex too (cheap relative to the actual file scan because the
///   newline scanner is `memchr`-fast for ASCII-compatible encodings). Future
///   refactor: stash a new backend with the same checkpoints + new encoding without
///   re-scanning.
/// - **Rebuild** otherwise: swap to a fresh ByteSeek so the viewport stays
///   interactive, then spawn a thread that rebuilds the LineIndex (or FullLoad)
///   under the new encoding. The thread follows the drain-and-swap-under-lock
///   protocol: drain `pending_grew`, optionally `extend_to`, `backend.store`, clear
///   `rebuilding`, all inside one `pending_grew` mutex critical section.
///
/// Returns immediately. The FE polls `get_session_status` for `is_indexing` and
/// switches its progress indicator like for the initial ByteSeek → LineIndex upgrade.
pub fn set_encoding(session_id: &str, new_encoding: FileEncoding) -> Result<(), ViewerError> {
    let path;
    let was_full_load;
    let current_encoding;
    let prev_cancel: Option<Arc<AtomicBool>>;
    {
        let sessions = SESSIONS.lock_ignore_poison();
        let session = sessions.get(session_id).ok_or_else(|| ViewerError::SessionNotFound {
            session_id: session_id.to_string(),
        })?;
        path = session.path.clone();
        was_full_load = matches!(*session.backend_type.lock_ignore_poison(), BackendType::FullLoad);
        current_encoding = *session.encoding.lock_ignore_poison();
        // Cancel any in-flight rebuild from a previous set_encoding call. The earlier
        // rebuild observes the flag and exits; the new rebuild owns the swap.
        prev_cancel = session.rebuilding.lock_ignore_poison().clone();
    }
    if current_encoding == new_encoding {
        return Ok(());
    }
    if let Some(cancel) = prev_cancel {
        cancel.store(true, Ordering::Relaxed);
    }

    // FullLoad path: reopen + atomic swap. Fast enough on <1 MB files that no
    // background thread is needed.
    if was_full_load {
        let new_backend: Box<dyn FileViewerBackend> =
            Box::new(FullLoadBackend::open_with_encoding(&path, new_encoding)?);
        let sessions = SESSIONS.lock_ignore_poison();
        if let Some(session) = sessions.get(session_id) {
            session.backend.store(Arc::new(new_backend));
            *session.encoding.lock_ignore_poison() = new_encoding;
        }
        return Ok(());
    }

    // Instant-swap path: when the byte layout is identical (same BOM + both
    // ASCII-newline-compatible), the existing LineIndex / ByteSeek is still
    // valid. We swap only the encoding field via `with_encoding`. No
    // background rebuild; the viewport stays unchanged except per-line decode.
    if same_byte_layout(current_encoding, new_encoding) {
        debug!(
            "set_encoding: instant swap {:?} -> {:?} (same byte layout)",
            current_encoding, new_encoding
        );
        let sessions = SESSIONS.lock_ignore_poison();
        if let Some(session) = sessions.get(session_id) {
            let backend = session.backend.load_full();
            if let Some(swapped) = backend.with_encoding(new_encoding) {
                session.backend.store(Arc::new(swapped));
                *session.encoding.lock_ignore_poison() = new_encoding;
                return Ok(());
            }
            // Backend declined the instant swap (e.g. FullLoad pre-decoded
            // its lines). Fall through to the rebuild path.
        }
    }

    // Large-file path: snap to ByteSeek immediately so the viewport stays interactive,
    // then rebuild LineIndex under the new encoding in the background.
    let bs = ByteSeekBackend::open_with_encoding(&path, new_encoding)?;
    let bs_box: Box<dyn FileViewerBackend> = Box::new(bs);
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let sessions = SESSIONS.lock_ignore_poison();
        if let Some(session) = sessions.get(session_id) {
            session.backend.store(Arc::new(bs_box));
            *session.backend_type.lock_ignore_poison() = BackendType::ByteSeek;
            *session.encoding.lock_ignore_poison() = new_encoding;
            *session.rebuilding.lock_ignore_poison() = Some(cancel.clone());
        }
    }

    let session_id_clone = session_id.to_string();
    let path_clone = path;
    let cancel_for_thread = cancel.clone();
    thread::spawn(move || {
        #[cfg(test)]
        {
            let hold = REBUILD_SLEEP_HOOK.swap(0, Ordering::SeqCst);
            if hold > 0 {
                thread::sleep(Duration::from_millis(hold));
            }
        }
        match LineIndexBackend::open_with_encoding(&path_clone, new_encoding, &cancel_for_thread) {
            Ok(new_backend) => {
                if cancel_for_thread.load(Ordering::Relaxed) {
                    return;
                }
                let sessions = SESSIONS.lock_ignore_poison();
                if let Some(session) = sessions.get(&session_id_clone) {
                    // Make sure we're still the owning rebuild before swapping.
                    let still_owner = session
                        .rebuilding
                        .lock_ignore_poison()
                        .as_ref()
                        .map(|c| Arc::ptr_eq(c, &cancel_for_thread))
                        .unwrap_or(false);
                    if !still_owner {
                        return;
                    }
                    // Drain-and-swap critical section.
                    let mut pending_lock = session.pending_grew.lock_ignore_poison();
                    let pending = pending_lock.take();
                    let final_backend: Box<dyn FileViewerBackend> = match pending {
                        Some(eof) if eof > new_backend.total_bytes() => {
                            match new_backend.extend_to(eof, &cancel_for_thread) {
                                Ok(extended) => Box::new(extended),
                                Err(_) => Box::new(new_backend),
                            }
                        }
                        _ => Box::new(new_backend),
                    };
                    session.backend.store(Arc::new(final_backend));
                    *session.backend_type.lock_ignore_poison() = BackendType::LineIndex;
                    *session.rebuilding.lock_ignore_poison() = None;
                    drop(pending_lock);
                }
            }
            Err(_) => {
                let sessions = SESSIONS.lock_ignore_poison();
                if let Some(session) = sessions.get(&session_id_clone) {
                    let still_owner = session
                        .rebuilding
                        .lock_ignore_poison()
                        .as_ref()
                        .map(|c| Arc::ptr_eq(c, &cancel_for_thread))
                        .unwrap_or(false);
                    if still_owner {
                        *session.rebuilding.lock_ignore_poison() = None;
                    }
                }
            }
        }
    });

    Ok(())
}

/// Test-only hook: simulates a watcher `Grew(eof)` event by writing into the session's
/// `pending_grew` queue. Drives `test_append_during_encoding_rebuild_not_dropped`
/// without standing up the (milestone-3) FS watcher.
#[cfg(test)]
pub fn test_only_push_pending_grew(session_id: &str, eof: u64) {
    let sessions = SESSIONS.lock_ignore_poison();
    if let Some(session) = sessions.get(session_id) {
        let mut q = session.pending_grew.lock_ignore_poison();
        let next = match *q {
            Some(prev) => prev.max(eof),
            None => eof,
        };
        *q = Some(next);
    }
}

/// Test-only hook: reads the current `pending_grew` queue value.
#[cfg(test)]
#[allow(dead_code, reason = "consumed by session_test race-coverage tests")]
pub fn test_only_pending_grew(session_id: &str) -> Option<u64> {
    let sessions = SESSIONS.lock_ignore_poison();
    sessions
        .get(session_id)
        .and_then(|s| *s.pending_grew.lock_ignore_poison())
}

/// Test-only hook: reads the rebuilding flag's strong-count parity (Some/None) to
/// help tests block-poll for completion without sleeping.
#[cfg(test)]
pub fn test_only_rebuilding_active(session_id: &str) -> bool {
    let sessions = SESSIONS.lock_ignore_poison();
    sessions
        .get(session_id)
        .map(|s| s.rebuilding.lock_ignore_poison().is_some())
        .unwrap_or(false)
}

/// Returns the count of currently-active reads. Test-only helper for asserting that
/// a cancelled or completed read cleaned up its `active_reads` entry.
#[cfg(test)]
pub fn active_read_count(session_id: &str) -> usize {
    let sessions = SESSIONS.lock_ignore_poison();
    let Some(session) = sessions.get(session_id) else {
        return 0;
    };
    session.active_reads.lock_ignore_poison().len()
}

/// Closes a viewer session and frees resources.
pub fn close_session(session_id: &str) -> Result<(), ViewerError> {
    // Drop any window→session mapping pointing at this session so the map
    // doesn't accumulate stale entries when the FE closes via the `viewer_close`
    // IPC (the common in-app close path). The map holds one entry per open
    // viewer window, so the linear scan is cheap.
    WINDOW_TO_SESSION
        .lock_ignore_poison()
        .retain(|_, sid| sid != session_id);

    let mut sessions = SESSIONS.lock_ignore_poison();
    if let Some(session) = sessions.remove(session_id) {
        // Cancel any ongoing search
        if let Some(search) = &session.search {
            search.cancel.store(true, Ordering::Relaxed);
        }
        // Cancel any ongoing upgrade
        if let Some(upgrade_cancel) = session.upgrading.lock_ignore_poison().as_ref() {
            upgrade_cancel.store(true, Ordering::Relaxed);
        }
        // Cancel any ongoing encoding rebuild
        if let Some(rebuild_cancel) = session.rebuilding.lock_ignore_poison().as_ref() {
            rebuild_cancel.store(true, Ordering::Relaxed);
        }
        // Stop the watcher manager thread; dropping the Arc<ViewerSubscription>
        // when `session` falls out of scope unregisters the underlying path.
        session.watcher_stop.store(true, Ordering::Relaxed);
        // Cancel any in-flight range reads so they exit promptly with `Cancelled`.
        for flag in session.active_reads.lock_ignore_poison().values() {
            flag.store(true, Ordering::Relaxed);
        }
        // Drop the `cmdr-media://` token (if any) so a closed-window viewer can't
        // leave a live token mapping a real path. This is the single choke point both
        // teardown paths (the `viewer_close` IPC and the `WindowEvent::Destroyed` net
        // via `close_session_for_window`) funnel through.
        if let Some(token) = &session.media_token {
            media::drop_token(token);
        }
    }
    Ok(())
}

/// Toggle tail mode for a session. When enabled, future watcher `Grew` events
/// trigger an `extend_to` on the active backend so the open viewport
/// auto-follows newly appended bytes. When disabled, the FE still receives
/// `viewer:file-changed:<sid>` events and renders its persistent reload toast.
pub fn set_tail_mode(session_id: &str, enabled: bool) -> Result<(), ViewerError> {
    let sessions = SESSIONS.lock_ignore_poison();
    let session = sessions.get(session_id).ok_or_else(|| ViewerError::SessionNotFound {
        session_id: session_id.to_string(),
    })?;
    session.tail_mode.store(enabled, Ordering::Relaxed);
    debug!("set_tail_mode: session={}, enabled={}", session_id, enabled);

    // If tail is being turned on and the file already grew on disk while tail
    // was off, jump the backend to the on-disk EOF so the user doesn't have to
    // wait for the next change to see the catch-up.
    if enabled {
        let path = session.path.clone();
        let backend_arc = session.load_backend();
        drop(sessions);
        if let Ok(meta) = std::fs::metadata(&path) {
            let on_disk = meta.len();
            if on_disk > backend_arc.total_bytes() {
                apply_tail_extend(session_id, on_disk);
            }
        }
    }
    Ok(())
}

/// Reopen the active backend from scratch with the session's current encoding.
/// Called by the FE's reload toast or by the watcher's rotation handler.
/// Choice of backend mirrors `open_session`: FullLoad under the threshold,
/// otherwise ByteSeek (an in-flight LineIndex upgrade isn't restarted here;
/// the next `get_lines` settles into the right backend).
pub fn reload(session_id: &str) -> Result<(), ViewerError> {
    let path;
    let encoding;
    {
        let sessions = SESSIONS.lock_ignore_poison();
        let session = sessions.get(session_id).ok_or_else(|| ViewerError::SessionNotFound {
            session_id: session_id.to_string(),
        })?;
        path = session.path.clone();
        encoding = *session.encoding.lock_ignore_poison();
    }

    let metadata = std::fs::metadata(&path)?;
    let file_size = metadata.len();
    let new_backend: Box<dyn FileViewerBackend> = if file_size <= FULL_LOAD_THRESHOLD {
        Box::new(FullLoadBackend::open_with_encoding(&path, encoding)?)
    } else {
        Box::new(ByteSeekBackend::open_with_encoding(&path, encoding)?)
    };
    let new_type = if file_size <= FULL_LOAD_THRESHOLD {
        BackendType::FullLoad
    } else {
        BackendType::ByteSeek
    };

    let sessions = SESSIONS.lock_ignore_poison();
    if let Some(session) = sessions.get(session_id) {
        session.backend.store(Arc::new(new_backend));
        *session.backend_type.lock_ignore_poison() = new_type;
        // Reset the pending grew queue; the fresh backend covers what the queue
        // was reserving for the old one.
        *session.pending_grew.lock_ignore_poison() = None;
    }
    Ok(())
}

/// Manager thread spawned once per session. Does the (blocking,
/// `fseventsd`-bound) FSEvents subscribe itself — off `open_session`'s critical
/// path — then owns the resulting `ViewerSubscription` (kept off `ViewerSession`
/// because the channel receiver isn't `Sync`) and runs the event loop. Drops the
/// subscription when `stop` flips (set by `close_session`) or when the upstream
/// channel disconnects; the subscription's `Drop` then unregisters the path from
/// the shared singleton.
fn spawn_watcher_manager(session_id: String, path: PathBuf, stop: Arc<AtomicBool>) {
    thread::spawn(move || {
        // `close_session` may have run before this thread got scheduled; skip
        // the blocking subscribe entirely in that case (nothing registered yet,
        // so nothing to unregister).
        if stop.load(Ordering::Relaxed) {
            return;
        }

        let sub = match VIEWER_WATCHER_MANAGER.subscribe(&path) {
            Ok(sub) => sub,
            Err(e) => {
                debug!("viewer watcher subscribe failed for {}: {}", path.display(), e);
                return;
            }
        };

        // `stop` may have flipped while we were subscribing. Returning here
        // drops `sub`, which unregisters the path.
        if stop.load(Ordering::Relaxed) {
            return;
        }

        catch_up_after_subscribe(&session_id, &path);

        // Poll with a timeout so we periodically check `stop` even if the
        // file's idle: this is the only way the manager exits when
        // `close_session` runs without the file changing first.
        const POLL: Duration = Duration::from_millis(200);
        loop {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            if let Some(event) = sub.recv_timeout(POLL) {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                handle_watcher_event(&session_id, event);
            }
        }
    });
}

/// Closes the open→subscribe missed-append window. Because the subscribe runs
/// in the background and blocks for an unbounded time, an append can land
/// between open and the watcher going live — and the watcher's size baseline is
/// the on-disk EOF *at subscribe time*, so that append would fire no event and
/// stay invisible. We compare the current on-disk size against what the active
/// backend covers and, if the file grew, drive the same path a real `Grew`
/// event would. Comparing against live backend coverage (rather than a captured
/// open-time EOF) makes this correct regardless of whether the ByteSeek →
/// LineIndex upgrade has already stored: mid-upgrade it queues into
/// `pending_grew` (drained by the swap), post-upgrade it tail-extends or emits.
fn catch_up_after_subscribe(session_id: &str, path: &Path) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    let on_disk = meta.len();
    let covered = {
        let sessions = SESSIONS.lock_ignore_poison();
        let Some(session) = sessions.get(session_id) else {
            return; // session closed while we were subscribing
        };
        session.backend.load_full().total_bytes()
    };
    if on_disk > covered {
        handle_watcher_event(session_id, WatcherEvent::Grew(on_disk));
    }
}

fn emit_file_changed(session_id: &str, kind: &'static str, new_size: Option<u64>) {
    let Some(handle) = app_handle() else {
        return;
    };
    let event = format!("viewer:file-changed:{}", session_id);
    let payload = serde_json::json!({
        "kind": kind,
        "newSize": new_size,
    });
    if let Err(e) = handle.emit(&event, payload) {
        debug!("emit viewer:file-changed failed: {}", e);
    }
}

fn handle_watcher_event(session_id: &str, event: WatcherEvent) {
    match event {
        WatcherEvent::MetadataOnly => {
            // No bytes changed; nothing for the viewer to do.
        }
        WatcherEvent::Grew(new_size) => {
            // Always tell the FE.
            emit_file_changed(session_id, "grew", Some(new_size));

            // Look up session state to decide whether to queue or apply.
            let (queue, can_extend, is_tail) = {
                let sessions = SESSIONS.lock_ignore_poison();
                let Some(session) = sessions.get(session_id) else {
                    return;
                };
                let upgrading = session.upgrading.lock_ignore_poison().is_some();
                let rebuilding = session.rebuilding.lock_ignore_poison().is_some();
                (
                    upgrading || rebuilding,
                    !upgrading && !rebuilding,
                    session.tail_mode.load(Ordering::Relaxed),
                )
            };

            if queue {
                push_pending_grew(session_id, new_size);
                return;
            }
            if can_extend && is_tail {
                apply_tail_extend(session_id, new_size);
            }
        }
        WatcherEvent::Shrunk | WatcherEvent::Replaced => {
            emit_file_changed(session_id, "rotated", None);
            // Best-effort reload; failure here surfaces on the next FE
            // interaction.
            let _ = reload(session_id);
        }
    }
}

/// Apply a single tail-mode extension under the drain-and-swap-under-lock
/// protocol.
///
/// Race protocol: `extend_to_boxed` can take seconds for a multi-MB append, so
/// we run it OUTSIDE the SESSIONS lock. That opens a window in which an
/// encoding rebuild or upgrade can install a fresh backend via
/// `ArcSwap::store`. Snapshotting the backend `Arc` before the long extend and
/// re-comparing via `Arc::ptr_eq` after — under the SESSIONS lock — lets us
/// detect that case and discard the stale extend instead of clobbering the
/// fresh backend. The new EOF is re-queued into `pending_grew` so the rebuild
/// swap or a follow-up watcher event still catches up.
fn apply_tail_extend(session_id: &str, new_size: u64) {
    let dummy_cancel = AtomicBool::new(false);

    let backend_snapshot = {
        let sessions = SESSIONS.lock_ignore_poison();
        let Some(session) = sessions.get(session_id) else {
            return;
        };
        // Re-check: an upgrade or rebuild may have started between the watcher
        // thread's read and this lock acquisition. Queue and bail in that case.
        if session.upgrading.lock_ignore_poison().is_some() || session.rebuilding.lock_ignore_poison().is_some() {
            let mut q = session.pending_grew.lock_ignore_poison();
            let next = match *q {
                Some(prev) => prev.max(new_size),
                None => new_size,
            };
            *q = Some(next);
            return;
        }
        let backend = session.backend.load_full();
        if new_size <= backend.total_bytes() {
            return;
        }
        backend
    };

    let extended = match backend_snapshot.extend_to_boxed(new_size, &dummy_cancel) {
        Ok(b) => b,
        Err(_) => {
            // The active backend can't extend (FullLoad). The viewer remains
            // valid against the older byte range until the user reloads.
            return;
        }
    };

    // Re-acquire the lock and verify the backend we extended is still the one
    // installed. If an encoding rebuild or upgrade swapped a new backend in
    // during our extend, our extended-from-stale backend would clobber it.
    let sessions = SESSIONS.lock_ignore_poison();
    let Some(session) = sessions.get(session_id) else {
        return;
    };
    let current = session.backend.load_full();
    if Arc::ptr_eq(&current, &backend_snapshot) {
        session.backend.store(Arc::new(extended));
    } else {
        // A fresh backend was installed during our extend. Discard the stale
        // extend and re-queue the EOF so the rebuild's drain-and-swap (or a
        // follow-up watcher event) picks it up. The new backend may already
        // cover `new_size` via its own drain; in that case the queue is
        // harmlessly higher than `total_bytes` and the next pass treats it as
        // a no-op.
        debug!(
            "apply_tail_extend: backend changed during extend; discarding stale extend for session {}",
            session_id
        );
        let mut q = session.pending_grew.lock_ignore_poison();
        let next = match *q {
            Some(prev) => prev.max(new_size),
            None => new_size,
        };
        *q = Some(next);
    }
}

fn push_pending_grew(session_id: &str, new_size: u64) {
    let sessions = SESSIONS.lock_ignore_poison();
    if let Some(session) = sessions.get(session_id) {
        let mut q = session.pending_grew.lock_ignore_poison();
        let next = match *q {
            Some(prev) => prev.max(new_size),
            None => new_size,
        };
        *q = Some(next);
    }
}

/// Test-only helper: drives the race in `apply_tail_extend` deterministically.
///
/// Simulates the timing the round-3 audit caught:
/// 1. The watcher thread takes a backend snapshot.
/// 2. Before its long `extend_to_boxed` returns, a separate concurrent
///    activity (encoding rebuild, upgrade) installs a brand-new backend.
/// 3. The watcher's eventual `store` must NOT clobber the new backend.
///
/// We script this by snapshotting the backend, calling `swap_callback` (which
/// the test uses to install a fresh backend via, e.g., `reload` or
/// `set_encoding`), then calling `extend_to_boxed` on the snapshot, then
/// running the same ptr-eq check the production code uses to decide
/// store-vs-discard. Returns `true` if the store was applied (snapshot still
/// current), `false` if the extend was discarded.
#[cfg(test)]
#[allow(dead_code, reason = "consumed by session_test tail-extend clobber-race test")]
pub fn test_only_run_tail_extend_with_swap(session_id: &str, new_size: u64, swap_callback: impl FnOnce()) -> bool {
    let dummy_cancel = AtomicBool::new(false);
    let backend_snapshot = {
        let sessions = SESSIONS.lock_ignore_poison();
        let session = sessions.get(session_id).expect("session must exist");
        session.backend.load_full()
    };
    // Trigger the racing swap (the test installs a new backend here).
    swap_callback();

    let extended = backend_snapshot
        .extend_to_boxed(new_size, &dummy_cancel)
        .expect("extend should succeed");

    let sessions = SESSIONS.lock_ignore_poison();
    let session = sessions.get(session_id).expect("session must exist");
    let current = session.backend.load_full();
    if Arc::ptr_eq(&current, &backend_snapshot) {
        session.backend.store(Arc::new(extended));
        true
    } else {
        let mut q = session.pending_grew.lock_ignore_poison();
        let next = match *q {
            Some(prev) => prev.max(new_size),
            None => new_size,
        };
        *q = Some(next);
        false
    }
}

/// Test-only helper: returns the current tail-mode flag.
#[cfg(test)]
#[allow(dead_code, reason = "consumed by session_test::tail_mode_can_be_toggled")]
pub fn test_only_tail_mode(session_id: &str) -> bool {
    let sessions = SESSIONS.lock_ignore_poison();
    sessions
        .get(session_id)
        .map(|s| s.tail_mode.load(Ordering::Relaxed))
        .unwrap_or(false)
}

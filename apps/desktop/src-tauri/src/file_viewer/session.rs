//! ViewerSession: orchestrates file viewer backends and manages session lifecycle.
//!
//! Opens a file, picks the right backend based on file size, and provides a session-based
//! API for the frontend. Sessions are cached by ID and cleaned up on close.

use cmdr_fs::log_error::LogError;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, RwLock};
use std::thread;
use std::time::Duration;

use super::materialize::PreviewTemp;
use super::pending_open::PendingOpen;
use crate::commands::file_system::expand_tilde;
use crate::ignore_poison::IgnorePoison;
use log::debug;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::byte_seek::ByteSeekBackend;
use super::content_kind::ViewerContentKind;
use super::encoding::{DETECT_HEAD_BYTES, FileEncoding, detect_from_head, same_byte_layout};
use super::full_load::FullLoadBackend;
use super::line_index::LineIndexBackend;
use super::markdown_session;
use super::media;
use super::media_session;
use super::origin::{SourceOrigin, open_for_viewer};
use super::range_read::{
    RangeEnd, RawSource, RowForm, STREAM_CHUNK_BYTES, Walked, read_range as do_read_range,
    read_range_streamed as stream_range,
};
use super::row_text::RowText;
use super::search_matcher::{Matcher, SearchMode};
use super::session_store::{
    BackendType, SESSIONS, SearchState, SearchStatus, ViewerOpenResult, ViewerSession, ViewerSessionInit,
    generate_session_id,
};
use super::source::{OpenedFile, read_head};
use super::watcher::{VIEWER_WATCHER_MANAGER, WatcherEvent};
use super::{
    FULL_LOAD_THRESHOLD, FileViewerBackend, LineChunk, MAX_SEARCH_MATCHES, SearchMatch, SeekTarget, TotalRows,
    ViewerError,
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

/// Current status of a viewer session.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ViewerSessionStatus {
    pub backend_type: BackendType,
    pub is_indexing: bool,
    /// How many ROWS the file has, and whether that is a count or an estimate. The
    /// frontend's scroll coordinate, so the poll that watches the index build is what
    /// swaps `ByteSeekBackend`'s estimate for `LineIndexBackend`'s real count.
    pub total_rows: TotalRows,
    /// Physical lines, if the backend knows them. The gutter's numbering and the
    /// status bar's count, ❌ never a row coordinate.
    pub total_lines: Option<usize>,
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
    // A window closed mid-pull has no session yet: stop the pull instead. Abandon
    // BEFORE reading the link, so an open that delivers in between has recorded it.
    super::pending_open::abandon_pending_open_for_window(window_label);
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

/// The biggest file read through its volume (a share, over smb2) that still gets the
/// background line index. Past it the scan can't finish inside
/// [`INDEXING_TIMEOUT_SECS`] on an ordinary link (32 MiB is about 0.3 s at 1 Gbit/s and
/// 2.7 s at 100 Mbit/s), so it would only compete with the viewport's own reads for
/// the link and the block cache before being cancelled.
const VOLUME_INDEX_MAX_BYTES: u64 = 32 * 1024 * 1024;

/// Opens a viewer session for the given file path.
///
/// Classifies the file by magic bytes first: an `Image` / `Pdf` on a local volume
/// opens as a media session (no text backend; the bytes are served via the
/// `cmdr-media://` scheme), everything else as a text session that picks a backend by
/// file size:
/// - Under 1 MB: FullLoad (instant, full random access)
/// - Over 1 MB: ByteSeek first (instant open), then upgrades to LineIndex in background
pub fn open_session(path: &str, volume_id: &str) -> Result<ViewerOpenResult, ViewerError> {
    open_session_inner(
        path,
        volume_id,
        /*force_text=*/ false,
        RowText::Raw,
        &PendingOpen::new(),
        None,
    )
}

/// Opens a fresh, full text session regardless of content kind. Backs the "View as
/// text" override: a media session isn't upgraded in place; the FE swaps to the
/// session this returns. Reuses the text path verbatim.
pub fn open_session_as_text(path: &str, volume_id: &str) -> Result<ViewerOpenResult, ViewerError> {
    open_session_inner(
        path,
        volume_id,
        /*force_text=*/ true,
        RowText::Raw,
        &PendingOpen::new(),
        None,
    )
}

/// Opens a session for the viewer window `window_label`, watched by `open`, and
/// links the window to it (so the window-destroyed handler frees it). `force_text`
/// is the "View as text" override; `rows` is how the session shows its rows (the debug
/// log's view hides log markers, `log_view.rs`).
///
/// A pull the open needs records its progress on `open` and stops once `open` is
/// abandoned (the window closed, or it stopped responding). An open abandoned after
/// its session was built closes that session and answers the abandon's error, so a
/// closed window never leaves a session behind. See `file_viewer::pending_open`.
///
/// A view switch reopens the same file for the same window, so when the window's
/// current session already holds a temp copy of it, the new session shares that copy
/// and pulls nothing. See `reusable_temp`.
pub fn open_for_window(
    path: &str,
    volume_id: &str,
    window_label: &str,
    force_text: bool,
    rows: RowText,
    open: &PendingOpen,
) -> Result<ViewerOpenResult, ViewerError> {
    let reuse = reusable_temp(window_label, path, volume_id);
    let result = open_session_inner(path, volume_id, force_text, rows, open, reuse)?;
    match open.deliver(|| register_window_session(window_label, &result.session_id)) {
        Ok(()) => Ok(result),
        Err(abandoned) => {
            let _ = close_session(&result.session_id);
            Err(abandoned)
        }
    }
}

/// The temp copy of `path` on `volume_id` that `window_label`'s current session holds,
/// if it holds one. Scoped to the window on purpose: another window on the same file
/// pulls its own copy, and a fresh open re-pulls, so only a view switch skips the pull.
fn reusable_temp(window_label: &str, path: &str, volume_id: &str) -> Option<Arc<PreviewTemp>> {
    if window_label.is_empty() {
        return None;
    }
    let session_id = WINDOW_TO_SESSION.lock_ignore_poison().get(window_label).cloned()?;
    let requested = PathBuf::from(expand_tilde(path));
    SESSIONS
        .lock_ignore_poison()
        .get(&session_id)?
        .temp
        .as_ref()
        .filter(|temp| temp.is_copy_of(&requested, volume_id))
        .cloned()
}

fn open_session_inner(
    path: &str,
    volume_id: &str,
    force_text: bool,
    rows: RowText,
    open: &PendingOpen,
    reuse: Option<Arc<PreviewTemp>>,
) -> Result<ViewerOpenResult, ViewerError> {
    // Wrapped rather than emitted inline: the body has a media early return and a
    // dozen `?`s, so an in-body emit would count a biased subset of opens. The
    // archive question is answered here from the pure path split (the same one
    // `rename_managed` uses) — the core's own answer needs the extraction to have
    // happened, which is exactly what the failure cases skip. It stays ARCHIVE-only
    // rather than "routed": `from_archive` is a shipped analytics dimension about
    // previewing inside a zip, and a portal open is honestly `false` for it.
    let from_archive = cmdr_fs::archive_format::archive_boundary_candidate(Path::new(&expand_tilde(path)))
        .is_some_and(|(_, inner)| !inner.as_os_str().is_empty());
    let result = open_session_core(path, volume_id, force_text, rows, open, reuse);
    super::analytics::emit_viewer_opened(&result, from_archive, force_text);
    result
}

fn open_session_core(
    path: &str,
    volume_id: &str,
    force_text: bool,
    rows: RowText,
    open: &PendingOpen,
    reuse: Option<Arc<PreviewTemp>>,
) -> Result<ViewerOpenResult, ViewerError> {
    let expanded = expand_tilde(path);
    let requested = PathBuf::from(&expanded);

    // A path a ROUTE serves (`/…/foo.zip/inner`, `/…/.git/branches/main/src/lib.rs`)
    // or a path on a volume the OS can't open (`adb://<serial>/sdcard/…`) has no
    // `std::fs` file, so the viewer can't touch it directly. Stream the file out to a
    // bounded temp and open THAT; any other path returns `None` and flows through
    // unchanged. Both are pulled through `volume_id`'s volume, not a hardcoded
    // `"root"`. A view switch hands in the copy its window already holds (`reuse`), so
    // it pulls nothing. The temp goes when the last session holding it closes, or right
    // here if this open fails. See `materialize`.
    //
    // Before both: a volume that holds a file open for positioned reads (SMB, through
    // smb2) serves it directly, mount or no mount, with no temp and no size cap. See
    // `volume_source`.
    let on_volume = match reuse {
        Some(_) => None,
        None => open_for_viewer(&requested, volume_id),
    };
    let temp = match (reuse, &on_volume) {
        (Some(temp), _) => Some(temp),
        (None, Some(_)) => None,
        (None, None) => super::materialize::materialize_for_viewer(&requested, volume_id, open)?
            .map(|file| Arc::new(PreviewTemp::new(file, requested.clone(), volume_id))),
    };
    let file_path = temp
        .as_ref()
        .map_or_else(|| requested.clone(), |temp| temp.temp_file.clone());

    // Stat and open ONCE: every backend this session builds reads through this handle.
    // A temp is a snapshot of a file the OS can't watch, and a volume-read file is
    // watchable only while its mount is there, so those decide the watcher too.
    let (origin, opened, watchable) = match on_volume {
        Some(found) => (found.origin, found.opened, found.os_visible),
        None => {
            if !file_path.exists() {
                return Err(ViewerError::NotFound { path: path.to_string() });
            }
            if file_path.is_dir() {
                return Err(ViewerError::IsDirectory);
            }
            let origin = SourceOrigin::Local(file_path.clone());
            let opened = origin.open()?;
            (origin, opened, temp.is_none())
        }
    };
    let file_size = opened.total_bytes;
    let on_a_volume = !matches!(origin, SourceOrigin::Local(_));

    // A Markdown file renders. Unlike media it needs no local path to serve, so it renders
    // from a share or a pulled temp too: the render reads through this session's handle.
    // See `markdown_session.rs`.
    if !force_text && markdown_session::is_markdown(&requested, &opened) {
        return Ok(markdown_session::open_markdown_session(
            markdown_session::MarkdownOpen {
                requested,
                volume_id: volume_id.to_string(),
                file_path,
                origin,
                opened,
                temp,
            },
        ));
    }

    // Classify by magic bytes (unless the caller forced text, e.g. "View as text").
    // A media kind (Image/Pdf on a local volume) opens a no-op session that serves bytes
    // via `cmdr-media://`; the whole media-open path lives in `media_session.rs`.
    // An extracted image/PDF renders inline too: the media session serves the temp via
    // `cmdr-media://` and holds the same shared `temp`, so its temp lives as long as the
    // session. `try_open_media` returns `None` (falls through to text) for non-media kinds.
    // A volume-read file has no local path to serve, and media renders only from a
    // local volume anyway (`media_session::is_local_posix_path`).
    if !force_text
        && !on_a_volume
        && let Some(result) = media_session::try_open_media(&file_path, file_size, temp.clone())
    {
        return result;
    }

    // Auto-detect encoding at open time. Used as the initial encoding for every backend.
    let detected_encoding = read_head(opened.source.as_ref(), opened.total_bytes, DETECT_HEAD_BYTES)
        .map(|head| detect_from_head(&head))
        .unwrap_or(FileEncoding::Utf8);

    let (backend_box, backend_type, upgrading): (Box<dyn FileViewerBackend>, BackendType, Option<Arc<AtomicBool>>) =
        if file_size <= FULL_LOAD_THRESHOLD {
            let b = FullLoadBackend::open_source(&opened, detected_encoding)?;
            (Box::new(b), BackendType::FullLoad, None)
        } else {
            // Start with ByteSeek (instant), then upgrade to LineIndex in background,
            // unless the file is on a volume and too big for the scan to finish.
            let b = ByteSeekBackend::open_source(&opened, detected_encoding)?;
            let index = !on_a_volume || file_size <= VOLUME_INDEX_MAX_BYTES;
            let upgrading = index.then(|| Arc::new(AtomicBool::new(false)));
            (Box::new(b), BackendType::ByteSeek, upgrading)
        };

    // Get initial lines
    // Never cancelled: the open's own deadline handling closes a late session whole.
    let mut initial_lines = backend_box.get_lines(&SeekTarget::Row(0), INITIAL_LINE_COUNT, &AtomicBool::new(false))?;
    rows.apply(&mut initial_lines);
    let capabilities = backend_box.capabilities();
    let total_bytes = backend_box.total_bytes();
    let total_lines = backend_box.total_lines();
    // The backend counts rows itself now, exactly or by its own bytes-per-row sample,
    // so there is nothing left to estimate here from the first chunk's string lengths.
    let estimated_total_rows = backend_box.total_rows().rows();
    let file_name = backend_box.file_name().to_string();

    let session_id = generate_session_id();
    let upgrade_cancel = upgrading.clone();
    let is_indexing = upgrade_cancel.is_some();

    let watcher_stop = Arc::new(AtomicBool::new(false));
    let watcher_stop_for_thread = watcher_stop.clone();

    let session = ViewerSession::new(ViewerSessionInit {
        backend: backend_box,
        backend_type: backend_type.clone(),
        upgrading: upgrade_cancel.clone(),
        encoding: detected_encoding,
        detected_encoding,
        watcher_stop,
        path: file_path.clone(),
        origin,
        file: Some(opened.clone()),
        media_token: None,
        temp,
        markdown: None,
        rows,
    });

    let result = ViewerOpenResult {
        session_id: session_id.clone(),
        file_name,
        total_bytes,
        total_lines,
        estimated_total_rows,
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
    // Tests can opt out via `CMDR_VIEWER_DISABLE_WATCHER=1`. A materialized temp is a
    // snapshot of a file the OS can't watch, so it gets no watcher (nothing to tail).
    if watchable && std::env::var("CMDR_VIEWER_DISABLE_WATCHER").is_err() {
        spawn_watcher_manager(session_id.clone(), session_path, watcher_stop_for_thread);
    }

    // If we're using ByteSeek, start background upgrade to LineIndex with timeout
    if let Some(cancel_flag) = upgrade_cancel {
        let session_id_clone = session_id.clone();
        let opened_for_indexer = opened;
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
            match LineIndexBackend::open_source(&opened_for_indexer, encoding_for_upgrade, &cancel_for_indexer) {
                Ok(new_backend) => {
                    // Test-only gate: park with the scan finished but the swap
                    // not yet done, the exact window a watcher `Grew` has to
                    // survive. Holds no lock. No-op unless a test armed it.
                    #[cfg(test)]
                    test_gate::UPGRADE_PRE_DRAIN.wait_if_armed();
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
                    debug!("Indexing failed for session {}: {}", session_id_clone, LogError(&e));
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
        total_rows: backend.total_rows(),
        total_lines: backend.total_lines(),
    })
}

/// Gets a range of lines from a session. `cancel` is the fetch's own flag (see
/// [`FileViewerBackend::get_lines`]).
pub fn get_lines(
    session_id: &str,
    target: SeekTarget,
    count: usize,
    cancel: &AtomicBool,
) -> Result<LineChunk, ViewerError> {
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
        session_id,
        backend_type,
        // allowed-log-private: a seek target (a row or byte), not a path
        target,
        count
    );

    backend.get_lines(&target, count, cancel)
}

/// Reads a bounded slice of the session's original bytes, without text decoding.
/// The session path also points at the shared temp for routed and device files.
pub fn get_bytes(session_id: &str, offset: u64, count: usize) -> Result<Vec<u8>, ViewerError> {
    const MAX_BYTES: usize = 64 * 1024;
    let file = {
        let sessions = SESSIONS.lock_ignore_poison();
        let session = sessions.get(session_id).ok_or_else(|| ViewerError::SessionNotFound {
            session_id: session_id.to_string(),
        })?;
        session.file_at_backend_size()?
    };
    let wanted = file.total_bytes.saturating_sub(offset).min(count.min(MAX_BYTES) as u64) as usize;
    let mut bytes = vec![0u8; wanted];
    let read = file.source.read_at(offset, &mut bytes)?;
    bytes.truncate(read);
    Ok(bytes)
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

    // The search walks the session's own open file, in the session's encoding, and matches each
    // row as the session shows it.
    let (file, encoding, matcher) = {
        let mut sessions = SESSIONS.lock_ignore_poison();
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| ViewerError::SessionNotFound {
                session_id: session_id.to_string(),
            })?;
        let file = session.file_at_backend_size()?;
        let encoding = *session.encoding.lock_ignore_poison();
        session.search = Some(search_state);
        (file, encoding, matcher.viewing(session.rows))
    };

    spawn_search_worker(file, encoding, matcher, cancel, matches, bytes_scanned, status);
    Ok(())
}

/// Spawns the worker thread that drives a single search. Factored out for clarity;
/// the worker's final status write must be under the same mutex critical section as
/// the watchdog's so a watchdog-set `Cancelled` is sticky (see step 1.4 of the
/// viewer-search plan).
fn spawn_search_worker(
    file: OpenedFile,
    encoding: FileEncoding,
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
        let backend = match ByteSeekBackend::open_source(&file, encoding) {
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

/// How often the search watchdog checks the worker's status and `cancel` flag.
pub(super) const SEARCH_WATCHDOG_POLL: Duration = Duration::from_millis(250);
/// How long the watchdog lets a worker ignore a set `cancel` flag before it
/// writes `Cancelled` itself.
pub(super) const SEARCH_WATCHDOG_BUDGET: Duration = Duration::from_secs(1);

/// Watchdog: polls the worker's `cancel` flag and forces the search status to
/// `Cancelled` if the worker hasn't observed the flag within 1 s. Exits as soon
/// as the worker writes a non-Running status (i.e. it finished naturally or got
/// cancelled cooperatively).
///
/// The poll + budget pair keeps user-visible cancellation under 1.25 s (plus
/// scheduling delay) in the worst case even for runaway-regex paths where the
/// inner `iter.next()` call doesn't observe the per-match cancel.
pub(super) fn run_search_watchdog(cancel: Arc<AtomicBool>, status: Arc<Mutex<SearchStatus>>) {
    let mut cancel_seen_at: Option<std::time::Instant> = None;
    loop {
        thread::sleep(SEARCH_WATCHDOG_POLL);
        // Cheap check first; bail out if the worker is done.
        let still_running = matches!(*status.lock_ignore_poison(), SearchStatus::Running);
        if !still_running {
            return;
        }
        if cancel.load(Ordering::Relaxed) {
            let started = cancel_seen_at.get_or_insert_with(std::time::Instant::now);
            if started.elapsed() >= SEARCH_WATCHDOG_BUDGET {
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
/// The whole range lands in memory here, which is why the copy dialogs cap what the
/// clipboard path will read; `write_range_to_file` streams instead. Locking and
/// cancellation plumbing: [`with_registered_read`].
pub fn read_range(session_id: &str, read_id: u64, anchor: RangeEnd, focus: RangeEnd) -> Result<String, ViewerError> {
    with_registered_read(session_id, read_id, |backend, cancel| {
        do_read_range(backend, anchor, focus, cancel)
    })
}

/// Runs `body` against the session's backend with `read_id` registered in
/// `active_reads`, so `cancel_read` (or session close) can stop it, and unregisters it
/// afterwards whether the read succeeded, failed, or was cancelled.
///
/// Holds the global SESSIONS lock only long enough to clone the backend `Arc` and
/// register the flag; `body` runs outside it, so other commands (`cancel_read`,
/// `get_session_status`, line fetches) stay responsive while a large read is in flight.
fn with_registered_read<T>(
    session_id: &str,
    read_id: u64,
    body: impl FnOnce(&dyn FileViewerBackend, &AtomicBool) -> Result<T, ViewerError>,
) -> Result<T, ViewerError> {
    let (backend, cancel_flag) = {
        let sessions = SESSIONS.lock_ignore_poison();
        let session = sessions.get(session_id).ok_or_else(|| ViewerError::SessionNotFound {
            session_id: session_id.to_string(),
        })?;
        let flag = Arc::new(AtomicBool::new(false));
        session.active_reads.lock_ignore_poison().insert(read_id, flag.clone());
        (session.load_backend(), flag)
    };

    let result = body(backend.as_ref().as_ref(), &cancel_flag);

    if let Ok(sessions) = SESSIONS.lock()
        && let Some(session) = sessions.get(session_id)
    {
        session.active_reads.lock_ignore_poison().remove(&read_id);
    }

    result
}

/// What a save reports while it runs, so a watcher can tell "slow" from "stopped".
///
/// A save has no honest upper bound on how long it may take (it's the way out of the
/// clipboard's size refusal, and the selection can be any size), so the IPC layer
/// watches it for silence rather than for elapsed time: it reads `bytes_written` on a
/// timer, and the count moving is the save's sign of life. See
/// `commands/file_viewer.rs` § `SAVE_STALL_LIMIT`.
#[derive(Debug, Default)]
pub struct SaveProgress {
    bytes_written: std::sync::atomic::AtomicU64,
    done: AtomicBool,
}

impl SaveProgress {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bytes handed to the destination so far.
    pub fn bytes_written(&self) -> u64 {
        self.bytes_written.load(Ordering::Relaxed)
    }

    /// True once the save has returned, whatever its outcome. A watcher about to give
    /// up on a silent save checks this first: the last chunk of a big save can take
    /// longer than the stall limit, and the silence that follows it is the save
    /// finishing, not stalling.
    pub fn is_done(&self) -> bool {
        self.done.load(Ordering::Relaxed)
    }

    fn note_written(&self, bytes: usize) {
        self.bytes_written.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    fn note_done(&self) {
        self.done.store(true, Ordering::Relaxed);
    }
}

/// What a finished save wrote, so the viewer can say so when it isn't the UTF-8 text
/// the user would expect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SavedAs {
    /// UTF-8 text: the range decoded cleanly, or the source is UTF-8 and the bytes its
    /// decode couldn't read are kept as they are.
    Utf8,
    /// The source's own bytes, in its own encoding (plus its BOM), because some
    /// characters in the range couldn't be decoded and UTF-8 would have replaced them.
    SourceEncoding { encoding: FileEncoding },
}

/// Streams a range to `dest_path`, decoded to UTF-8 and written atomically. Uses the
/// same `read_id` cancellation plumbing as `read_range`. Write is temp+rename for
/// crash-safety: if the process dies mid-write, the user keeps their original file (if
/// any) instead of a half-written one.
///
/// Memory is one [`STREAM_CHUNK_BYTES`] buffer, not one range: this is the way out the
/// copy dialog offers when the clipboard refuses a selection for being too big, so
/// reading the range into a `String` here would allocate exactly what the refusal is
/// protecting the user from.
///
/// The bytes go through the same decode as an on-screen read, so a UTF-16 (or any
/// other non-UTF-8) source saves as UTF-8 text. Chunks break on line boundaries, so no
/// character is ever split across two writes. Where the selection holds a character
/// the decode lost (U+FFFD for bytes it couldn't read), the save keeps the source's
/// bytes instead: a UTF-8 source writes them in place, so a stray Latin-1 or binary
/// byte past the detection head reaches the copy unchanged; any other source starts
/// over and writes the whole range in its own encoding, answering
/// [`SavedAs::SourceEncoding`]. `DETAILS.md` § "A save keeps bytes the decode lost".
///
/// On success, returns how the range was written. On `Cancelled`, and on any other error, the partial
/// temp file is best-effort removed and the typed error is returned, so a stopped save
/// leaves nothing behind. The cancel flag is read once more right before the rename:
/// a stall watch that gave up (and said so) while the last write hung must not see the
/// destination change after its answer.
///
/// Reports each write into `progress`, and marks it done on every exit: that's what
/// lets the IPC layer watch a save for silence instead of holding it to a total
/// deadline it has no honest way to meet.
pub fn write_range_to_file(
    session_id: &str,
    read_id: u64,
    anchor: RangeEnd,
    focus: RangeEnd,
    dest_path: &Path,
    progress: &SaveProgress,
) -> Result<SavedAs, ViewerError> {
    let result = write_range_to_file_inner(session_id, read_id, anchor, focus, dest_path, progress);
    progress.note_done();
    result
}

fn write_range_to_file_inner(
    session_id: &str,
    read_id: u64,
    anchor: RangeEnd,
    focus: RangeEnd,
    dest_path: &Path,
    progress: &SaveProgress,
) -> Result<SavedAs, ViewerError> {
    // Atomic write: write to a `<dest>.cmdr-tmp-<uuid>` sibling, then rename. The
    // same-FS rename gives us atomicity on local volumes (and is best-effort
    // elsewhere). The house mint names it, so a name at the filesystem's limit
    // still gets a temp that fits, and the pane hides it while it's written.
    let staged = cmdr_fs::staging::StagingTemp::mint(dest_path, None);
    let tmp_path = staged.path();

    let source = SaveSource::of(session_id)?;
    let read = |start: u64, end: u64| source.read(start, end);
    let raw = RawSource {
        read: &read,
        encoding: source.encoding,
    };

    let landed = with_registered_read(session_id, read_id, |backend, cancel| {
        // Text first, watching for lost characters. A walk that meets one in a
        // non-UTF-8 source starts over writing the source's own bytes, and one whose
        // source bytes stopped matching its rows starts over as plain text: at most
        // three walks, each streamed, so memory stays one chunk.
        let mut form = RowForm::TextKeepingLoss(raw);
        let saved = loop {
            // Unbuffered on purpose: each piece the reader hands over is already a
            // chunk, so a `BufWriter` would only copy it a second time. `create`
            // truncates, so a walk that starts over starts on an empty temp.
            let mut file = std::fs::File::create(tmp_path)?;
            if let RowForm::Source(_) = form {
                // The rows start past the BOM; without it, a UTF-16 copy reads as noise.
                file.write_all(source.encoding.bom_bytes())?;
            }
            let mut sink = |piece: &[u8]| {
                file.write_all(piece)?;
                progress.note_written(piece.len());
                Ok(())
            };
            let walked = stream_range(
                backend,
                anchor.clone(),
                focus.clone(),
                cancel,
                STREAM_CHUNK_BYTES,
                form,
                &mut sink,
            )?;
            form = match (walked, form) {
                (Walked::Finished, RowForm::Source(_)) => {
                    break SavedAs::SourceEncoding {
                        encoding: source.encoding,
                    };
                }
                (Walked::Finished, _) => break SavedAs::Utf8,
                (Walked::Lossy, _) => RowForm::Source(raw),
                (Walked::SourceChanged, _) => RowForm::Text,
            };
        };
        // Still registered, so a `cancel_read` that raced the last write lands here.
        if cancel.load(Ordering::Relaxed) {
            return Err(ViewerError::Cancelled);
        }
        std::fs::rename(tmp_path, dest_path).map_err(|e| ViewerError::Io {
            message: e.to_string().into(),
        })?;
        Ok(saved)
    });

    if landed.is_err() {
        // Best-effort cleanup; ignore secondary errors.
        let _ = std::fs::remove_file(tmp_path);
    }
    landed
}

/// Where a save reads the source's own bytes from, opened only if a row asks: a save
/// whose decode is clean never does.
struct SaveSource {
    session_id: String,
    encoding: FileEncoding,
    opened: std::cell::OnceCell<Result<OpenedFile, ViewerError>>,
}

impl SaveSource {
    fn of(session_id: &str) -> Result<Self, ViewerError> {
        let sessions = SESSIONS.lock_ignore_poison();
        let session = sessions.get(session_id).ok_or_else(|| ViewerError::SessionNotFound {
            session_id: session_id.to_string(),
        })?;
        Ok(Self {
            session_id: session_id.to_string(),
            encoding: *session.encoding.lock_ignore_poison(),
            opened: std::cell::OnceCell::new(),
        })
    }

    /// The source's bytes in `[start, end)`.
    fn read(&self, start: u64, end: u64) -> Result<Vec<u8>, ViewerError> {
        let opened = self.opened.get_or_init(|| {
            let sessions = SESSIONS.lock_ignore_poison();
            let session = sessions
                .get(&self.session_id)
                .ok_or_else(|| ViewerError::SessionNotFound {
                    session_id: self.session_id.clone(),
                })?;
            session.file_at_backend_size()
        });
        let file = opened.as_ref().map_err(Clone::clone)?;
        let len = usize::try_from(end.saturating_sub(start)).map_err(|_| ViewerError::OutOfRange)?;
        let mut bytes = vec![0u8; len];
        let mut filled = 0;
        while filled < len {
            let read = file.source.read_at(start + filled as u64, &mut bytes[filled..])?;
            if read == 0 {
                break;
            }
            filled += read;
        }
        bytes.truncate(filled);
        Ok(bytes)
    }
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
    let file;
    let was_full_load;
    let current_encoding;
    let prev_cancel: Option<Arc<AtomicBool>>;
    {
        let sessions = SESSIONS.lock_ignore_poison();
        let session = sessions.get(session_id).ok_or_else(|| ViewerError::SessionNotFound {
            session_id: session_id.to_string(),
        })?;
        file = session.file_at_backend_size()?;
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
        let new_backend: Box<dyn FileViewerBackend> = Box::new(FullLoadBackend::open_source(&file, new_encoding)?);
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
    let bs = ByteSeekBackend::open_source(&file, new_encoding)?;
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
    let cancel_for_thread = cancel.clone();
    thread::spawn(move || {
        #[cfg(test)]
        let _exit_guard = RebuildExitGuard;
        #[cfg(test)]
        test_gate::REBUILD_PRE_SCAN.wait_if_armed();
        match LineIndexBackend::open_source(&file, new_encoding, &cancel_for_thread) {
            Ok(new_backend) => {
                // See the upgrade thread: parks with the scan done, before the
                // drain-and-swap. Holds no lock.
                #[cfg(test)]
                test_gate::REBUILD_PRE_DRAIN.wait_if_armed();
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
                    #[cfg(test)]
                    test_gate::note_rebuild_store();
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
        // A Markdown session's image tokens go the same way, and its image pulls stop.
        if let Some(document) = &session.markdown {
            document.close();
        }
        // `session` drops at the end of this block, and with it its share of the
        // preview temp (if any): `PreviewTemp`'s `Drop` removes the subdir once the
        // last session sharing it (a view switch's twin) has closed.
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
        let origin = session.origin.clone();
        let backend_arc = session.load_backend();
        drop(sessions);
        if let Ok(on_disk) = origin.current_size()
            && on_disk > backend_arc.total_bytes()
        {
            apply_tail_extend(session_id, on_disk);
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
    let origin;
    let encoding;
    {
        let sessions = SESSIONS.lock_ignore_poison();
        let session = sessions.get(session_id).ok_or_else(|| ViewerError::SessionNotFound {
            session_id: session_id.to_string(),
        })?;
        origin = session.origin.clone();
        encoding = *session.encoding.lock_ignore_poison();
    }

    let (new_backend, new_type, opened) = reopen_backend(&origin, encoding)?;

    let sessions = SESSIONS.lock_ignore_poison();
    if let Some(session) = sessions.get(session_id) {
        *session.file.lock_ignore_poison() = Some(opened);
        session.backend.store(Arc::new(new_backend));
        *session.backend_type.lock_ignore_poison() = new_type;
        // Reset the pending grew queue; the fresh backend covers what the queue
        // was reserving for the old one.
        *session.pending_grew.lock_ignore_poison() = None;
    }
    Ok(())
}

/// A fresh handle and backend for the file as it is now: FullLoad under the
/// threshold, ByteSeek above it. A fresh handle, not the session's: a file replaced
/// under the session (an atomic save, a log rotation) is a different file.
fn reopen_backend(
    origin: &SourceOrigin,
    encoding: FileEncoding,
) -> Result<(Box<dyn FileViewerBackend>, BackendType, OpenedFile), ViewerError> {
    let opened = origin.open()?;
    if opened.total_bytes <= FULL_LOAD_THRESHOLD {
        let backend = FullLoadBackend::open_source(&opened, encoding)?;
        Ok((Box::new(backend), BackendType::FullLoad, opened))
    } else {
        let backend = ByteSeekBackend::open_source(&opened, encoding)?;
        Ok((Box::new(backend), BackendType::ByteSeek, opened))
    }
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
                debug!(
                    "viewer watcher subscribe failed for {}: {}",
                    cmdr_fs::log_private::LogPrivate::file_path(&path),
                    e
                );
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
///
/// FullLoad can't extend in place, so it reopens instead, through the same
/// snapshot-and-compare: still FullLoad while the file fits, ByteSeek once it
/// crosses the threshold. Without this, tail mode does nothing under 1 MB.
fn apply_tail_extend(session_id: &str, new_size: u64) {
    let dummy_cancel = AtomicBool::new(false);

    let (backend_snapshot, reopen_from) = {
        let sessions = SESSIONS.lock_ignore_poison();
        let Some(session) = sessions.get(session_id) else {
            return;
        };
        // Re-check: an upgrade or rebuild may have started between the watcher
        // thread's read and this lock acquisition. Queue and bail in that case.
        if session.upgrading.lock_ignore_poison().is_some() || session.rebuilding.lock_ignore_poison().is_some() {
            coalesce_pending_grew(session, new_size);
            return;
        }
        let backend = session.backend.load_full();
        if new_size <= backend.total_bytes() {
            return;
        }
        let reopen_from = matches!(*session.backend_type.lock_ignore_poison(), BackendType::FullLoad)
            .then(|| (session.origin.clone(), *session.encoding.lock_ignore_poison()));
        (backend, reopen_from)
    };

    let (extended, new_type, reopened) = match reopen_from {
        Some((origin, encoding)) => match reopen_backend(&origin, encoding) {
            Ok((backend, backend_type, opened)) => (backend, Some(backend_type), Some(opened)),
            Err(e) => {
                debug!(
                    "apply_tail_extend: reopening session {session_id} failed: {e}",
                    e = LogError(&e)
                );
                return;
            }
        },
        None => match backend_snapshot.extend_to_boxed(new_size, &dummy_cancel) {
            Ok(b) => (b, None, None),
            Err(e) => {
                debug!(
                    "apply_tail_extend: extending session {session_id} failed: {e}",
                    e = LogError(&e)
                );
                return;
            }
        },
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
        if let Some(new_type) = new_type {
            *session.backend_type.lock_ignore_poison() = new_type;
        }
        if let Some(opened) = reopened {
            *session.file.lock_ignore_poison() = Some(opened);
        }
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
        coalesce_pending_grew(session, new_size);
    }
}

/// Queue an EOF for the next drain, coalescing with whatever is already there.
///
/// Always keeps the LARGER EOF: the queue means "the file is at least this
/// big", so a later, smaller event must never shrink it. Every queue write
/// goes through here, including the test hook, so the rule has one home.
///
/// Takes the session ref rather than the id because most callers already hold
/// the `SESSIONS` lock; `push_pending_grew` is the id-taking wrapper.
fn coalesce_pending_grew(session: &ViewerSession, new_size: u64) {
    let mut q = session.pending_grew.lock_ignore_poison();
    let next = match *q {
        Some(prev) => prev.max(new_size),
        None => new_size,
    };
    *q = Some(next);
}

fn push_pending_grew(session_id: &str, new_size: u64) {
    let sessions = SESSIONS.lock_ignore_poison();
    if let Some(session) = sessions.get(session_id) {
        coalesce_pending_grew(session, new_size);
    }
}

#[cfg(test)]
pub mod test_hooks;
/// Re-exported so the gate points above and every test keep referring to
/// `session::test_gate` / `session::test_only_*` unqualified.
#[cfg(test)]
pub use test_hooks::*;

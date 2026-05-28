//! ViewerSession: orchestrates file viewer backends and manages session lifecycle.
//!
//! Opens a file, picks the right backend based on file size, and provides a session-based
//! API for the frontend. Sessions are cached by ID and cleaned up on close.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::Duration;

use crate::commands::file_system::expand_tilde;
use crate::ignore_poison::IgnorePoison;
use log::debug;
use serde::Serialize;

use super::byte_seek::ByteSeekBackend;
use super::full_load::FullLoadBackend;
use super::line_index::LineIndexBackend;
use super::range_read::{RangeEnd, read_range as do_read_range};
use super::search_matcher::{Matcher, MatcherBuildError, SearchMode};
use super::{
    BackendCapabilities, FULL_LOAD_THRESHOLD, FileViewerBackend, LineChunk, MAX_SEARCH_MATCHES, SearchMatch,
    SeekTarget, ViewerError,
};

/// Which backend strategy is active for a session.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum BackendType {
    FullLoad,
    ByteSeek,
    LineIndex,
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
    /// `Arc` so a long-running range read can clone the pointer and drop the SESSIONS
    /// lock while the read iterates lines (which can take seconds for 100 MB selections).
    /// The upgrade thread replaces the Arc when ByteSeek -> LineIndex completes.
    backend: Arc<dyn FileViewerBackend>,
    backend_type: BackendType,
    search: Option<SearchState>,
    /// Set when upgrading from ByteSeek to LineIndex in the background.
    upgrading: Option<Arc<AtomicBool>>,
    /// Per-read cancel flags. Each `read_range` call inserts an entry keyed by the FE's
    /// `read_id`, removes it on completion (cancelled or not). The FE generates fresh
    /// ids per call (a monotonic counter is fine; uniqueness within a session is all
    /// that's needed). Per-read (not session-wide) so a cancel for one read doesn't
    /// affect a follow-up read that started in the same gesture.
    active_reads: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    path: PathBuf,
}

/// Global session cache.
static SESSIONS: LazyLock<Mutex<HashMap<String, ViewerSession>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

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
/// Picks the backend based on file size:
/// - Under 1 MB: FullLoad (instant, full random access)
/// - Over 1 MB: ByteSeek first (instant open), then upgrades to LineIndex in background
pub fn open_session(path: &str) -> Result<ViewerOpenResult, ViewerError> {
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

    let (backend, backend_type, upgrading): (Arc<dyn FileViewerBackend>, BackendType, Option<Arc<AtomicBool>>) =
        if file_size <= FULL_LOAD_THRESHOLD {
            let b = FullLoadBackend::open(&file_path)?;
            (Arc::new(b), BackendType::FullLoad, None)
        } else {
            // Start with ByteSeek (instant), then upgrade to LineIndex in background
            let b = ByteSeekBackend::open(&file_path)?;
            let cancel = Arc::new(AtomicBool::new(false));
            (Arc::new(b), BackendType::ByteSeek, Some(cancel))
        };

    // Get initial lines
    let initial_lines = backend.get_lines(&SeekTarget::Line(0), INITIAL_LINE_COUNT)?;
    let capabilities = backend.capabilities();
    let total_bytes = backend.total_bytes();
    let total_lines = backend.total_lines();
    let file_name = backend.file_name().to_string();

    let session_id = generate_session_id();

    // If we're using ByteSeek, start background upgrade to LineIndex with timeout
    let upgrade_cancel = upgrading.clone();
    let is_indexing = upgrade_cancel.is_some();
    if let Some(cancel_flag) = upgrade_cancel.clone() {
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
                if let Ok(mut sessions) = SESSIONS.lock()
                    && let Some(session) = sessions.get_mut(&session_id_for_timeout)
                {
                    session.upgrading = None;
                }
            }
        });

        // Spawn indexing thread
        thread::spawn(move || {
            match LineIndexBackend::open(&path_clone, &cancel_for_indexer) {
                Ok(new_backend) => {
                    if !cancel_for_indexer.load(Ordering::Relaxed) {
                        let mut sessions = SESSIONS.lock_ignore_poison();
                        if let Some(session) = sessions.get_mut(&session_id_clone) {
                            debug!(
                                "Indexing completed for session {}, upgrading to LineIndex",
                                session_id_clone
                            );
                            session.backend = Arc::new(new_backend);
                            session.backend_type = BackendType::LineIndex;
                            session.upgrading = None;
                        }
                    }
                }
                Err(e) => {
                    debug!("Indexing failed for session {}: {}", session_id_clone, e);
                    // Mark as no longer indexing
                    if let Ok(mut sessions) = SESSIONS.lock()
                        && let Some(session) = sessions.get_mut(&session_id_clone)
                    {
                        session.upgrading = None;
                    }
                }
            }
        });
    }

    let session = ViewerSession {
        backend,
        backend_type: backend_type.clone(),
        search: None,
        upgrading: upgrade_cancel,
        active_reads: Mutex::new(HashMap::new()),
        path: file_path,
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
    };

    SESSIONS.lock_ignore_poison().insert(session_id, session);

    Ok(result)
}

/// Gets the current status of a session (backend type, indexing state).
pub fn get_session_status(session_id: &str) -> Result<ViewerSessionStatus, ViewerError> {
    let sessions = SESSIONS.lock_ignore_poison();
    let session = sessions.get(session_id).ok_or_else(|| ViewerError::SessionNotFound {
        session_id: session_id.to_string(),
    })?;

    Ok(ViewerSessionStatus {
        backend_type: session.backend_type.clone(),
        is_indexing: session.upgrading.is_some(),
        total_lines: session.backend.total_lines(),
    })
}

/// Gets a range of lines from a session.
pub fn get_lines(session_id: &str, target: SeekTarget, count: usize) -> Result<LineChunk, ViewerError> {
    let sessions = SESSIONS.lock_ignore_poison();
    let session = sessions.get(session_id).ok_or_else(|| ViewerError::SessionNotFound {
        session_id: session_id.to_string(),
    })?;

    debug!(
        "get_lines: session={}, backend_type={:?}, target={:?}, count={}",
        session_id, session.backend_type, target, count
    );

    session.backend.get_lines(&target, count)
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
            let message = match err {
                MatcherBuildError::InvalidRegex(msg) => format!("Invalid regex: {}", msg),
                MatcherBuildError::MultilineNotSupported => {
                    "Multiline patterns aren't supported. The viewer searches line by line.".to_string()
                }
            };
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

    let total_bytes = session.backend.total_bytes();

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
        (session.backend.clone(), flag)
    };

    let result = do_read_range(backend.as_ref(), anchor, focus, &cancel_flag);

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
    dest_path: &std::path::Path,
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
    let mut sessions = SESSIONS.lock_ignore_poison();
    if let Some(session) = sessions.remove(session_id) {
        // Cancel any ongoing search
        if let Some(search) = &session.search {
            search.cancel.store(true, Ordering::Relaxed);
        }
        // Cancel any ongoing upgrade
        if let Some(upgrade_cancel) = &session.upgrading {
            upgrade_cancel.store(true, Ordering::Relaxed);
        }
        // Cancel any in-flight range reads so they exit promptly with `Cancelled`.
        for flag in session.active_reads.lock_ignore_poison().values() {
            flag.store(true, Ordering::Relaxed);
        }
    }
    Ok(())
}

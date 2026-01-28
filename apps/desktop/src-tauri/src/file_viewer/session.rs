//! ViewerSession — orchestrates file viewer backends and manages session lifecycle.
//!
//! Opens a file, picks the right backend based on file size, and provides a session-based
//! API for the frontend. Sessions are cached by ID and cleaned up on close.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::Duration;

use log::debug;
use serde::Serialize;

use super::byte_seek::ByteSeekBackend;
use super::full_load::FullLoadBackend;
use super::line_index::LineIndexBackend;
use super::{
    BackendCapabilities, FULL_LOAD_THRESHOLD, FileViewerBackend, LineChunk, SearchMatch, SeekTarget, ViewerError,
};

/// Which backend strategy is active for a session.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BackendType {
    FullLoad,
    ByteSeek,
    LineIndex,
}

/// Result returned when opening a viewer session.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerOpenResult {
    pub session_id: String,
    pub file_name: String,
    pub total_bytes: u64,
    pub total_lines: Option<usize>,
    /// Estimated total lines based on initial sample (for ByteSeek where total_lines is unknown).
    /// Calculated as total_bytes / avg_bytes_per_line from initial lines.
    pub estimated_total_lines: usize,
    pub backend_type: BackendType,
    pub capabilities: BackendCapabilities,
    /// Initial chunk of lines from the start of the file.
    pub initial_lines: LineChunk,
    /// Whether background indexing is in progress (for ByteSeek -> LineIndex upgrade).
    pub is_indexing: bool,
}

/// Current status of a viewer session.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerSessionStatus {
    pub backend_type: BackendType,
    pub is_indexing: bool,
    pub total_lines: Option<usize>,
}

/// Status of an ongoing search.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchStatus {
    /// Search is running. `bytes_scanned` shows progress.
    Running,
    /// Search completed.
    Done,
    /// Search was cancelled.
    Cancelled,
    /// No search is active.
    Idle,
}

/// Result from polling search progress.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPollResult {
    pub status: SearchStatus,
    pub matches: Vec<SearchMatch>,
    pub total_bytes: u64,
    pub bytes_scanned: u64,
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
    backend: Box<dyn FileViewerBackend>,
    backend_type: BackendType,
    search: Option<SearchState>,
    /// Set when upgrading from ByteSeek to LineIndex in the background.
    upgrading: Option<Arc<AtomicBool>>,
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

/// Expands tilde (~) to the user's home directory.
fn expand_tilde(path: &str) -> String {
    if (path.starts_with("~/") || path == "~")
        && let Some(home) = dirs::home_dir()
    {
        return path.replacen("~", &home.to_string_lossy(), 1);
    }
    path.to_string()
}

/// Opens a viewer session for the given file path.
/// Picks the backend based on file size:
/// - Under 1 MB: FullLoad (instant, full random access)
/// - Over 1 MB: ByteSeek first (instant open), then upgrades to LineIndex in background
pub fn open_session(path: &str) -> Result<ViewerOpenResult, ViewerError> {
    let expanded = expand_tilde(path);
    let file_path = PathBuf::from(&expanded);

    if !file_path.exists() {
        return Err(ViewerError::NotFound(path.to_string()));
    }
    if file_path.is_dir() {
        return Err(ViewerError::IsDirectory);
    }

    let metadata = std::fs::metadata(&file_path)?;
    let file_size = metadata.len();

    let (backend, backend_type, upgrading): (Box<dyn FileViewerBackend>, BackendType, Option<Arc<AtomicBool>>) =
        if file_size <= FULL_LOAD_THRESHOLD {
            let b = FullLoadBackend::open(&file_path)?;
            (Box::new(b), BackendType::FullLoad, None)
        } else {
            // Start with ByteSeek (instant), then upgrade to LineIndex in background
            let b = ByteSeekBackend::open(&file_path)?;
            let cancel = Arc::new(AtomicBool::new(false));
            (Box::new(b), BackendType::ByteSeek, Some(cancel))
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
                        let mut sessions = SESSIONS.lock().unwrap();
                        if let Some(session) = sessions.get_mut(&session_id_clone) {
                            debug!(
                                "Indexing completed for session {}, upgrading to LineIndex",
                                session_id_clone
                            );
                            session.backend = Box::new(new_backend);
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
        if avg_bytes_per_line > 0 {
            (total_bytes as usize) / avg_bytes_per_line
        } else {
            (total_bytes as usize) / 80 // fallback
        }
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

    SESSIONS.lock().unwrap().insert(session_id, session);

    Ok(result)
}

/// Gets the current status of a session (backend type, indexing state).
pub fn get_session_status(session_id: &str) -> Result<ViewerSessionStatus, ViewerError> {
    let sessions = SESSIONS.lock().unwrap();
    let session = sessions
        .get(session_id)
        .ok_or(ViewerError::SessionNotFound(session_id.to_string()))?;

    Ok(ViewerSessionStatus {
        backend_type: session.backend_type.clone(),
        is_indexing: session.upgrading.is_some(),
        total_lines: session.backend.total_lines(),
    })
}

/// Gets a range of lines from a session.
pub fn get_lines(session_id: &str, target: SeekTarget, count: usize) -> Result<LineChunk, ViewerError> {
    let sessions = SESSIONS.lock().unwrap();
    let session = sessions
        .get(session_id)
        .ok_or(ViewerError::SessionNotFound(session_id.to_string()))?;

    debug!(
        "get_lines: session={}, backend_type={:?}, target={:?}, count={}",
        session_id, session.backend_type, target, count
    );

    session.backend.get_lines(&target, count)
}

/// Starts a background search in the given session.
/// Any previous search is cancelled first.
pub fn search_start(session_id: &str, query: String) -> Result<(), ViewerError> {
    // First, cancel any existing search
    search_cancel(session_id)?;

    let cancel = Arc::new(AtomicBool::new(false));
    let matches: Arc<Mutex<Vec<SearchMatch>>> = Arc::new(Mutex::new(Vec::new()));
    let bytes_scanned: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));
    let status: Arc<Mutex<SearchStatus>> = Arc::new(Mutex::new(SearchStatus::Running));

    let search_state = SearchState {
        cancel: cancel.clone(),
        matches: matches.clone(),
        bytes_scanned: bytes_scanned.clone(),
        status: status.clone(),
    };

    // Get the file path from the session to open a fresh file handle in the search thread
    let path = {
        let mut sessions = SESSIONS.lock().unwrap();
        let session = sessions
            .get_mut(session_id)
            .ok_or(ViewerError::SessionNotFound(session_id.to_string()))?;
        session.search = Some(search_state);
        session.path.clone()
    };

    // Spawn search thread — creates its own backend for searching
    let cancel_clone = cancel.clone();
    let matches_clone = matches;
    let bytes_scanned_clone = bytes_scanned;
    let status_clone = status;

    thread::spawn(move || {
        // Use ByteSeekBackend for streaming search (low memory, works on any file)
        let backend = match ByteSeekBackend::open(&path) {
            Ok(b) => b,
            Err(_) => {
                *status_clone.lock().unwrap() = SearchStatus::Done;
                return;
            }
        };

        let result = backend.search(&query, &cancel_clone, &matches_clone);
        let final_scanned: u64 = result.unwrap_or_default();

        *bytes_scanned_clone.lock().unwrap() = final_scanned;

        let final_status = if cancel_clone.load(Ordering::Relaxed) {
            SearchStatus::Cancelled
        } else {
            SearchStatus::Done
        };
        *status_clone.lock().unwrap() = final_status;
    });

    Ok(())
}

/// Polls search progress for a session.
pub fn search_poll(session_id: &str) -> Result<SearchPollResult, ViewerError> {
    let sessions = SESSIONS.lock().unwrap();
    let session = sessions
        .get(session_id)
        .ok_or(ViewerError::SessionNotFound(session_id.to_string()))?;

    let total_bytes = session.backend.total_bytes();

    match &session.search {
        None => Ok(SearchPollResult {
            status: SearchStatus::Idle,
            matches: Vec::new(),
            total_bytes,
            bytes_scanned: 0,
        }),
        Some(search) => {
            let status = search.status.lock().unwrap().clone();
            let matches = search.matches.lock().unwrap().clone();
            let bytes_scanned = *search.bytes_scanned.lock().unwrap();

            Ok(SearchPollResult {
                status,
                matches,
                total_bytes,
                bytes_scanned,
            })
        }
    }
}

/// Cancels an ongoing search.
pub fn search_cancel(session_id: &str) -> Result<(), ViewerError> {
    let mut sessions = SESSIONS.lock().unwrap();
    let session = sessions
        .get_mut(session_id)
        .ok_or(ViewerError::SessionNotFound(session_id.to_string()))?;

    if let Some(search) = &session.search {
        search.cancel.store(true, Ordering::Relaxed);
    }
    session.search = None;

    Ok(())
}

/// Closes a viewer session and frees resources.
pub fn close_session(session_id: &str) -> Result<(), ViewerError> {
    let mut sessions = SESSIONS.lock().unwrap();
    if let Some(session) = sessions.remove(session_id) {
        // Cancel any ongoing search
        if let Some(search) = &session.search {
            search.cancel.store(true, Ordering::Relaxed);
        }
        // Cancel any ongoing upgrade
        if let Some(upgrade_cancel) = &session.upgrading {
            upgrade_cancel.store(true, Ordering::Relaxed);
        }
    }
    Ok(())
}

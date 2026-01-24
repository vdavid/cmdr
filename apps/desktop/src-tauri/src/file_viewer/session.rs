//! ViewerSession — orchestrates file viewer backends and manages session lifecycle.
//!
//! Opens a file, picks the right backend based on file size, and provides a session-based
//! API for the frontend. Sessions are cached by ID and cleaned up on close.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;

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
    pub backend_type: BackendType,
    pub capabilities: BackendCapabilities,
    /// Initial chunk of lines from the start of the file.
    pub initial_lines: LineChunk,
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

    // If we're using ByteSeek, start background upgrade to LineIndex
    let upgrade_cancel = upgrading.clone();
    if upgrade_cancel.is_some() {
        let session_id_clone = session_id.clone();
        let path_clone = file_path.clone();
        let cancel_clone = upgrade_cancel.clone().unwrap();

        thread::spawn(move || {
            let cancel_flag = &cancel_clone;
            match LineIndexBackend::open(&path_clone, cancel_flag) {
                Ok(new_backend) => {
                    if !cancel_flag.load(Ordering::Relaxed) {
                        let mut sessions = SESSIONS.lock().unwrap();
                        if let Some(session) = sessions.get_mut(&session_id_clone) {
                            session.backend = Box::new(new_backend);
                            session.backend_type = BackendType::LineIndex;
                            session.upgrading = None;
                        }
                    }
                }
                Err(_) => {
                    // If upgrade fails, keep using ByteSeek — it still works fine
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

    let result = ViewerOpenResult {
        session_id: session_id.clone(),
        file_name,
        total_bytes,
        total_lines,
        backend_type,
        capabilities,
        initial_lines,
    };

    SESSIONS.lock().unwrap().insert(session_id, session);

    Ok(result)
}

/// Gets a range of lines from a session.
pub fn get_lines(session_id: &str, target: SeekTarget, count: usize) -> Result<LineChunk, ViewerError> {
    let sessions = SESSIONS.lock().unwrap();
    let session = sessions
        .get(session_id)
        .ok_or(ViewerError::SessionNotFound(session_id.to_string()))?;
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

//! Streaming directory listing: types, state, and async implementation.
//!
//! Provides non-blocking directory reading with progress events and cancellation.
//! The implementation spawns background tasks and emits Tauri events.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, RwLock};

use crate::benchmark;
use crate::file_system::listing::caching::{CachedListing, LISTING_CACHE};
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder, sort_entries};
use crate::file_system::volume::VolumeError;
use crate::file_system::volume::friendly_error::{
    FriendlyError, enrich_with_provider, friendly_error_from_volume_error,
};
use crate::file_system::watcher::start_watching;

// ============================================================================
// Types and state
// ============================================================================

/// Status of a streaming directory listing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum ListingStatus {
    Loading,
    Ready,
    Cancelled,
    Error { message: String },
}

/// Result of starting a streaming directory listing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamingListingStartResult {
    pub listing_id: String,
    /// Always `Loading`.
    pub status: ListingStatus,
}

/// Progress event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingProgressEvent {
    pub listing_id: String,
    pub loaded_count: usize,
}

/// Completion event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingCompleteEvent {
    pub listing_id: String,
    pub total_count: usize,
    pub max_filename_width: Option<f32>,
    /// Root path of the volume this listing belongs to
    pub volume_root: String,
}

/// Error event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingErrorEvent {
    pub listing_id: String,
    pub message: String,
    /// Structured error info when available. `None` for internal errors (task panics).
    pub friendly: Option<FriendlyError>,
}

/// Cancelled event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingCancelledEvent {
    pub listing_id: String,
}

/// Read-complete event payload (emitted when read_dir finishes, before sorting/caching)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingReadCompleteEvent {
    pub listing_id: String,
    pub total_count: usize,
}

/// Opening event payload (emitted just before read_dir starts - the slow part for network folders)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingOpeningEvent {
    pub listing_id: String,
}

/// State for an in-progress streaming listing
pub struct StreamingListingState {
    /// Checked at sync cancellation points (before read, after read, at cache insert).
    pub cancelled: AtomicBool,
    /// Async signal for `select!`-based cancellation during the listing I/O.
    pub cancel_notify: tokio::sync::Notify,
}

/// Cache for streaming state (separate from completed listings cache)
pub(crate) static STREAMING_STATE: LazyLock<RwLock<HashMap<String, Arc<StreamingListingState>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

// ============================================================================
// Event sink trait (decouples streaming from Tauri)
// ============================================================================

/// Abstraction over listing event emission.
/// Production: `TauriListingEventSink` wraps `AppHandle`.
/// Tests: `CollectorListingEventSink` stores events for assertion.
pub(crate) trait ListingEventSink: Send + Sync {
    fn emit_opening(&self, listing_id: &str);
    fn emit_progress(&self, listing_id: &str, loaded_count: usize);
    fn emit_read_complete(&self, listing_id: &str, total_count: usize);
    fn emit_complete(&self, listing_id: &str, total_count: usize, max_filename_width: Option<f32>, volume_root: String);
    fn emit_error(&self, listing_id: &str, message: String, friendly: Option<FriendlyError>);
    fn emit_cancelled(&self, listing_id: &str);
}

/// Tauri-backed listing event sink — calls `app.emit()` for each event.
pub(crate) struct TauriListingEventSink {
    app: tauri::AppHandle,
}

impl TauriListingEventSink {
    pub(crate) fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl ListingEventSink for TauriListingEventSink {
    fn emit_opening(&self, listing_id: &str) {
        use tauri::Emitter;
        let _ = self.app.emit(
            "listing-opening",
            ListingOpeningEvent {
                listing_id: listing_id.to_string(),
            },
        );
    }

    fn emit_progress(&self, listing_id: &str, loaded_count: usize) {
        use tauri::Emitter;
        let _ = self.app.emit(
            "listing-progress",
            ListingProgressEvent {
                listing_id: listing_id.to_string(),
                loaded_count,
            },
        );
    }

    fn emit_read_complete(&self, listing_id: &str, total_count: usize) {
        use tauri::Emitter;
        let _ = self.app.emit(
            "listing-read-complete",
            ListingReadCompleteEvent {
                listing_id: listing_id.to_string(),
                total_count,
            },
        );
    }

    fn emit_complete(
        &self,
        listing_id: &str,
        total_count: usize,
        max_filename_width: Option<f32>,
        volume_root: String,
    ) {
        use tauri::Emitter;
        let _ = self.app.emit(
            "listing-complete",
            ListingCompleteEvent {
                listing_id: listing_id.to_string(),
                total_count,
                max_filename_width,
                volume_root,
            },
        );
    }

    fn emit_error(&self, listing_id: &str, message: String, friendly: Option<FriendlyError>) {
        use tauri::Emitter;
        let _ = self.app.emit(
            "listing-error",
            ListingErrorEvent {
                listing_id: listing_id.to_string(),
                message,
                friendly,
            },
        );
    }

    fn emit_cancelled(&self, listing_id: &str) {
        use tauri::Emitter;
        let _ = self.app.emit(
            "listing-cancelled",
            ListingCancelledEvent {
                listing_id: listing_id.to_string(),
            },
        );
    }
}

/// Test listing event sink — stores events for inspection.
#[cfg(test)]
pub(crate) struct CollectorListingEventSink {
    pub opening: std::sync::Mutex<Vec<String>>,
    pub progress: std::sync::Mutex<Vec<(String, usize)>>,
    pub read_complete: std::sync::Mutex<Vec<(String, usize)>>,
    pub complete: std::sync::Mutex<Vec<(String, usize)>>,
    pub errors: std::sync::Mutex<Vec<(String, String)>>,
    pub cancelled: std::sync::Mutex<Vec<String>>,
}

#[cfg(test)]
impl CollectorListingEventSink {
    pub fn new() -> Self {
        Self {
            opening: std::sync::Mutex::new(Vec::new()),
            progress: std::sync::Mutex::new(Vec::new()),
            read_complete: std::sync::Mutex::new(Vec::new()),
            complete: std::sync::Mutex::new(Vec::new()),
            errors: std::sync::Mutex::new(Vec::new()),
            cancelled: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[cfg(test)]
impl ListingEventSink for CollectorListingEventSink {
    fn emit_opening(&self, listing_id: &str) {
        self.opening.lock().unwrap().push(listing_id.to_string());
    }

    fn emit_progress(&self, listing_id: &str, loaded_count: usize) {
        self.progress
            .lock()
            .unwrap()
            .push((listing_id.to_string(), loaded_count));
    }

    fn emit_read_complete(&self, listing_id: &str, total_count: usize) {
        self.read_complete
            .lock()
            .unwrap()
            .push((listing_id.to_string(), total_count));
    }

    fn emit_complete(
        &self,
        listing_id: &str,
        total_count: usize,
        _max_filename_width: Option<f32>,
        _volume_root: String,
    ) {
        self.complete
            .lock()
            .unwrap()
            .push((listing_id.to_string(), total_count));
    }

    fn emit_error(&self, listing_id: &str, message: String, _friendly: Option<FriendlyError>) {
        self.errors.lock().unwrap().push((listing_id.to_string(), message));
    }

    fn emit_cancelled(&self, listing_id: &str) {
        self.cancelled.lock().unwrap().push(listing_id.to_string());
    }
}

// ============================================================================
// Streaming implementation
// ============================================================================

/// Starts a streaming directory listing that returns immediately and emits progress events.
///
/// This is non-blocking - the actual directory reading happens in a background task.
/// Progress is reported via Tauri events every ~200ms.
#[allow(
    clippy::too_many_arguments,
    reason = "Streaming operation requires many state parameters"
)]
pub async fn list_directory_start_streaming(
    app: tauri::AppHandle,
    volume_id: &str,
    path: &Path,
    include_hidden: bool,
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
    listing_id: String,
) -> Result<StreamingListingStartResult, std::io::Error> {
    // Reset benchmark epoch for this navigation
    benchmark::reset_epoch();
    benchmark::log_event_value("list_directory_start_streaming CALLED", path.display());

    // Create streaming state with cancellation flag
    let state = Arc::new(StreamingListingState {
        cancelled: AtomicBool::new(false),
        cancel_notify: tokio::sync::Notify::new(),
    });

    // Store state for cancellation
    if let Ok(mut cache) = STREAMING_STATE.write() {
        cache.insert(listing_id.clone(), Arc::clone(&state));
    }

    // Clone values for the spawned task
    let listing_id_for_spawn = listing_id.clone();
    let path_owned = path.to_path_buf();
    let path_for_error = path.to_path_buf();
    let volume_id_owned = volume_id.to_string();
    let events: Arc<dyn ListingEventSink> = Arc::new(TauriListingEventSink::new(app));

    // Spawn background task
    tokio::spawn(async move {
        // Clone again for use after the listing call
        let listing_id_for_cleanup = listing_id_for_spawn.clone();
        let events_for_error = Arc::clone(&events);

        let result = read_directory_with_progress(
            &events,
            &listing_id_for_spawn,
            &state,
            &volume_id_owned,
            &path_owned,
            include_hidden,
            sort_by,
            sort_order,
            dir_sort_mode,
        )
        .await;

        // Clean up streaming state
        if let Ok(mut cache) = STREAMING_STATE.write() {
            cache.remove(&listing_id_for_cleanup);
        }

        // Handle task result
        match result {
            Err(e) => {
                // Function returned an error (volume not found, permission denied, I/O, etc.)
                let mut friendly = friendly_error_from_volume_error(&e, &path_for_error);
                enrich_with_provider(&mut friendly, &path_for_error);
                events_for_error.emit_error(&listing_id_for_cleanup, e.to_string(), Some(friendly));
            }
            Ok(()) => {
                // Success - read_directory_with_progress already emitted listing-complete
            }
        }
    });

    benchmark::log_event("list_directory_start_streaming RETURNING");
    Ok(StreamingListingStartResult {
        listing_id,
        status: ListingStatus::Loading,
    })
}

/// Reads a directory with progress reporting.
///
/// Async implementation that spawns the Volume I/O in a background task
/// and uses `tokio::select!` with a cancellation polling loop for responsive ESC handling.
/// Uses the Volume abstraction to support both local filesystem and MTP devices.
#[allow(
    clippy::too_many_arguments,
    reason = "Streaming operation requires many state parameters"
)]
pub(crate) async fn read_directory_with_progress(
    events: &Arc<dyn ListingEventSink>,
    listing_id: &str,
    state: &Arc<StreamingListingState>,
    volume_id: &str,
    path: &Path,
    include_hidden: bool,
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
) -> Result<(), VolumeError> {
    benchmark::log_event("read_directory_with_progress START");
    log::debug!(
        "read_directory_with_progress: listing_id={}, volume_id={}, path={}",
        listing_id,
        volume_id,
        path.display()
    );

    // Emit opening event - this is the slow part for network folders
    // (SMB connection establishment, directory handle creation, MTP queries)
    events.emit_opening(listing_id);

    // Check cancellation before starting
    if state.cancelled.load(Ordering::Relaxed) {
        benchmark::log_event("read_directory_with_progress CANCELLED (before read)");
        events.emit_cancelled(listing_id);
        return Ok(());
    }

    // Get the volume from VolumeManager
    let volume = crate::file_system::get_volume_manager()
        .get(volume_id)
        .ok_or_else(|| VolumeError::NotFound(format!("Volume not found: {}", volume_id)))?;

    // Read directory entries via Volume abstraction.
    // Spawn the listing as a tokio task and use select! with a cancellation poll loop
    // to remain responsive even when filesystem I/O blocks (slow/stuck network drives).
    let read_start = std::time::Instant::now();
    let path_for_task = path.to_path_buf();
    let events_for_progress = Arc::clone(events);
    let listing_id_for_progress = listing_id.to_string();

    let mut listing_task = tokio::spawn(async move {
        let on_progress = |loaded_count: usize| {
            events_for_progress.emit_progress(&listing_id_for_progress, loaded_count);
        };
        volume.list_directory(&path_for_task, Some(&on_progress)).await
    });

    // Wait for either listing completion or cancellation — no polling.
    let entries_result = tokio::select! {
        biased;  // check cancellation first if both are ready
        _ = state.cancel_notify.notified() => {
            benchmark::log_event("read_directory_with_progress CANCELLED (during read_dir)");
            listing_task.abort();
            events.emit_cancelled(listing_id);
            return Ok(());
        }
        result = &mut listing_task => {
            result.map_err(|e| VolumeError::IoError {
                message: format!("Directory listing task failed: {}", e),
                raw_os_error: None,
            })?
        }
    };

    let mut entries = entries_result?;
    let read_dir_time = read_start.elapsed();
    benchmark::log_event_value("read_dir COMPLETE, entries", entries.len());

    // Emit read-complete event (before sorting/caching) so UI can show "All N files loaded"
    events.emit_read_complete(listing_id, entries.len());

    // Check cancellation one more time before finalizing
    if state.cancelled.load(Ordering::Relaxed) {
        benchmark::log_event("read_directory_with_progress CANCELLED (after read)");
        events.emit_cancelled(listing_id);
        return Ok(());
    }

    // Enrich directory entries with index data (recursive_size etc.) before sorting,
    // so that sort-by-size works correctly for directories.
    crate::indexing::enrich_entries_with_index(&mut entries);
    crate::indexing::trigger_verification(&path.to_string_lossy());

    // Sort entries
    benchmark::log_event("sort START");
    sort_entries(&mut entries, sort_by, sort_order, dir_sort_mode);
    benchmark::log_event("sort END");

    // Calculate counts based on include_hidden setting
    let total_count = if include_hidden {
        entries.len()
    } else {
        entries.iter().filter(|e| !e.name.starts_with('.')).count()
    };

    // Calculate max filename width if font metrics are available
    let max_filename_width = {
        let font_id = "system-400-12"; // Default font (must match list_directory_start_with_volume)
        let filenames: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        crate::font_metrics::calculate_max_width(&filenames, font_id)
    };

    // Cache the completed listing, with atomic cancellation check.
    // We check cancellation WHILE holding the cache lock to avoid a race condition:
    // without this, a cancel arriving between a check and insert would leave a stale
    // entry (listDirectoryEnd would try to remove before the entry exists, then this
    // insert would add it permanently).
    if let Ok(mut cache) = LISTING_CACHE.write() {
        // Check cancellation while holding the lock - makes check+insert atomic
        if state.cancelled.load(Ordering::Relaxed) {
            benchmark::log_event("read_directory_with_progress CANCELLED (at cache insert)");
            events.emit_cancelled(listing_id);
            return Ok(());
        }

        cache.insert(
            listing_id.to_string(),
            CachedListing {
                volume_id: volume_id.to_string(),
                path: path.to_path_buf(),
                entries,
                sort_by,
                sort_order,
                directory_sort_mode: dir_sort_mode,
                sequence: std::sync::atomic::AtomicU64::new(0),
            },
        );
    }

    // Get the volume from VolumeManager to check if it supports watching
    if let Some(volume) = crate::file_system::get_volume_manager().get(volume_id)
        && volume.supports_watching()
        && let Err(e) = start_watching(listing_id, path)
    {
        log::warn!("Failed to start watcher: {}", e);
        // Continue anyway - watcher is optional enhancement
    }

    // Get volume root for the event (used by frontend to determine if at volume root)
    let volume_root = crate::file_system::get_volume_manager()
        .get(volume_id)
        .map(|v| v.root().to_string_lossy().to_string())
        .unwrap_or_else(|| "/".to_string());

    // Emit completion event
    events.emit_complete(listing_id, total_count, max_filename_width, volume_root);

    benchmark::log_event_value(
        "read_directory_with_progress COMPLETE, read_dir_time_ms",
        read_dir_time.as_millis(),
    );
    Ok(())
}

/// Cancels an in-progress streaming listing.
///
/// Sets the cancellation flag, which will be checked by the background task.
pub fn cancel_listing(listing_id: &str) {
    if let Ok(cache) = STREAMING_STATE.read()
        && let Some(state) = cache.get(listing_id)
    {
        state.cancelled.store(true, Ordering::Relaxed);
        state.cancel_notify.notify_waiters();
        benchmark::log_event_value("cancel_listing", listing_id);
    }
}

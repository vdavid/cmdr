//! Streaming directory listing: types, state, and async implementation.
//!
//! Provides non-blocking directory reading with progress events and cancellation.
//! The implementation spawns background tasks and emits Tauri events.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, RwLock};
use tauri_specta::Event;

use crate::benchmark;
use crate::file_system::listing::caching::{CachedListing, LISTING_CACHE};
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder, sort_entries};
use crate::file_system::volume::VolumeError;
use crate::file_system::volume::friendly_error::{
    ListingError, enrich_with_provider, listing_error_for_restricted_empty_root, listing_error_from_volume_error,
};
use crate::file_system::watcher::start_watching;
#[cfg(test)]
use crate::ignore_poison::IgnorePoison;

// ============================================================================
// Types and state
// ============================================================================

/// Status of a streaming directory listing
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "status")]
pub enum ListingStatus {
    Loading,
    Ready,
    Cancelled,
    Error { message: String },
}

/// Result of starting a streaming directory listing
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StreamingListingStartResult {
    pub listing_id: String,
    /// Always `Loading`.
    pub status: ListingStatus,
}

/// Progress event payload
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "listing-progress")]
pub struct ListingProgressEvent {
    pub listing_id: String,
    pub loaded_count: usize,
}

/// Completion event payload
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "listing-complete")]
pub struct ListingCompleteEvent {
    pub listing_id: String,
    pub total_count: usize,
    /// Root path of the volume this listing belongs to
    pub volume_root: String,
}

/// Error event payload
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "listing-error")]
pub struct ListingErrorEvent {
    pub listing_id: String,
    /// Raw message kept for the FE's non-display logic (MTP fallback, deleted-path
    /// detection); the user-facing copy is rendered from `error`.
    pub message: String,
    /// Typed, word-free classification when available. `None` for internal errors
    /// (task panics). The FE renders all copy from this.
    pub error: Option<ListingError>,
}

/// Cancelled event payload
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "listing-cancelled")]
pub struct ListingCancelledEvent {
    pub listing_id: String,
}

/// Read-complete event payload (emitted when read_dir finishes, before sorting/caching)
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "listing-read-complete")]
pub struct ListingReadCompleteEvent {
    pub listing_id: String,
    pub total_count: usize,
}

/// Opening event payload (emitted just before read_dir starts - the slow part for network folders)
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "listing-opening")]
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
    fn emit_complete(&self, listing_id: &str, total_count: usize, volume_root: String);
    fn emit_error(&self, listing_id: &str, message: String, error: Option<ListingError>);
    fn emit_cancelled(&self, listing_id: &str);
}

/// Tauri-backed listing event sink: calls `app.emit()` for each event.
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
        let _ = ListingOpeningEvent {
            listing_id: listing_id.to_string(),
        }
        .emit(&self.app);
    }

    fn emit_progress(&self, listing_id: &str, loaded_count: usize) {
        let _ = ListingProgressEvent {
            listing_id: listing_id.to_string(),
            loaded_count,
        }
        .emit(&self.app);
    }

    fn emit_read_complete(&self, listing_id: &str, total_count: usize) {
        let _ = ListingReadCompleteEvent {
            listing_id: listing_id.to_string(),
            total_count,
        }
        .emit(&self.app);
    }

    fn emit_complete(&self, listing_id: &str, total_count: usize, volume_root: String) {
        let _ = ListingCompleteEvent {
            listing_id: listing_id.to_string(),
            total_count,
            volume_root,
        }
        .emit(&self.app);
    }

    fn emit_error(&self, listing_id: &str, message: String, error: Option<ListingError>) {
        // PII-free analytics: a listing failed with a categorized error. Only the
        // category enum crosses; never the path, message, or any provider detail.
        if let Some(e) = &error {
            let category = serde_json::to_value(e.category)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string));
            if let Some(category) = category {
                crate::analytics::posthog::capture("error_encountered", serde_json::json!({ "category": category }));
            }
        }
        let _ = ListingErrorEvent {
            listing_id: listing_id.to_string(),
            message,
            error,
        }
        .emit(&self.app);
    }

    fn emit_cancelled(&self, listing_id: &str) {
        let _ = ListingCancelledEvent {
            listing_id: listing_id.to_string(),
        }
        .emit(&self.app);
    }
}

/// Test listing event sink: stores events for inspection.
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
        self.opening.lock_ignore_poison().push(listing_id.to_string());
    }

    fn emit_progress(&self, listing_id: &str, loaded_count: usize) {
        self.progress
            .lock_ignore_poison()
            .push((listing_id.to_string(), loaded_count));
    }

    fn emit_read_complete(&self, listing_id: &str, total_count: usize) {
        self.read_complete
            .lock_ignore_poison()
            .push((listing_id.to_string(), total_count));
    }

    fn emit_complete(&self, listing_id: &str, total_count: usize, _volume_root: String) {
        self.complete
            .lock_ignore_poison()
            .push((listing_id.to_string(), total_count));
    }

    fn emit_error(&self, listing_id: &str, message: String, _error: Option<ListingError>) {
        self.errors.lock_ignore_poison().push((listing_id.to_string(), message));
    }

    fn emit_cancelled(&self, listing_id: &str) {
        self.cancelled.lock_ignore_poison().push(listing_id.to_string());
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
                let mut listing_error = listing_error_from_volume_error(&e, &path_for_error);
                enrich_with_provider(&mut listing_error, &path_for_error);
                if matches!(&e, VolumeError::PermissionDenied(_)) {
                    crate::restricted_paths::record_denial(&path_for_error);
                }
                let message = e.to_string();
                // Record into the MCP recent-errors ring buffer so `cmdr://state`
                // surfaces what just failed, without callers grepping the log file.
                crate::mcp::listing_errors::record(
                    &listing_id_for_cleanup,
                    &volume_id_owned,
                    &path_for_error.to_string_lossy(),
                    &message,
                );
                events_for_error.emit_error(&listing_id_for_cleanup, message, Some(listing_error));
            }
            Ok(()) => {
                // Success: listing-complete already emitted. If this path was
                // previously recorded as TCC-restricted, drop it: the user
                // must have granted access (per-folder TCC popup or FDA toggle).
                crate::restricted_paths::clear_path(&path_for_error);
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

    // Resolve the volume, routing a `.zip`-crossing path to its read-only
    // `ArchiveVolume`. The cache still keys on the FE-provided `volume_id` (the
    // parent drive), so the downstream re-read sites re-resolve the same archive
    // from `(volume_id, path)` — eviction-safe (see `caching.rs`).
    let resolved = crate::file_system::get_volume_manager().resolve(volume_id, path);
    let is_archive = resolved.is_archive;
    let volume = resolved
        .volume
        .ok_or_else(|| VolumeError::NotFound(format!("Volume not found: {}", volume_id)))?;
    // The listing task consumes its own handle; keep `volume` for the watcher
    // check and `volume_root` below (an archive's `root()` is the `.zip` path).
    let volume_for_task = Arc::clone(&volume);

    // Read directory entries via Volume abstraction.
    // Spawn the listing as a tokio task and use select! with a cancellation poll loop
    // to remain responsive even when filesystem I/O blocks (slow/stuck network drives).
    let total_start = std::time::Instant::now();
    let read_start = std::time::Instant::now();
    let path_for_task = path.to_path_buf();
    let events_for_progress = Arc::clone(events);
    let listing_id_for_progress = listing_id.to_string();

    let mut listing_task = tokio::spawn(async move {
        // Stall-probe: marker logged as the FIRST executable line inside the spawned task.
        // If `read_directory_with_progress: entry` fires but this `task started` line doesn't,
        // the tokio runtime didn't schedule this task (worker starvation). If both fire but
        // `list_directory_core` doesn't follow, the Volume's list_directory itself is blocked.
        // Info-level (matches the other `stall_probe::*` lifecycle markers); always lands in
        // the prod file chain for organic-repro triage.
        log::info!(
            target: "stall_probe::listing_task",
            "task started: listing_id={}, path={}",
            listing_id_for_progress,
            path_for_task.display(),
        );
        let on_progress = |p: crate::file_system::volume::ListingProgress| {
            // Streaming listing UI shows "Loaded N entries…", so it wants total
            // entry count, not just files. `ListingProgress::entries()` sums
            // files + dirs for that.
            events_for_progress.emit_progress(&listing_id_for_progress, p.entries());
        };
        volume_for_task.list_directory(&path_for_task, Some(&on_progress)).await
    });

    // Wait for either listing completion or cancellation (no polling).
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
    // so that sort-by-size works correctly for directories. Archives have no drive
    // index (their inner paths aren't real FS paths), so enrich/verify are skipped.
    let enrich_start = std::time::Instant::now();
    if !is_archive {
        crate::indexing::enrich_entries_with_index_on_volume(volume_id, &mut entries);
        crate::indexing::trigger_verification(volume_id, &path.to_string_lossy());
    }
    let enrich_ms = enrich_start.elapsed().as_millis();

    // Sort entries
    benchmark::log_event("sort START");
    let sort_start = std::time::Instant::now();
    sort_entries(&mut entries, sort_by, sort_order, dir_sort_mode);
    let sort_ms = sort_start.elapsed().as_millis();
    benchmark::log_event("sort END");

    // Calculate counts based on include_hidden setting
    let total_count = if include_hidden {
        entries.len()
    } else {
        entries.iter().filter(|e| !e.name.starts_with('.')).count()
    };

    // Cache the completed listing, with atomic cancellation check.
    // We check cancellation WHILE holding the cache lock to avoid a race condition:
    // without this, a cancel arriving between a check and insert would leave a stale
    // entry (listDirectoryEnd would try to remove before the entry exists, then this
    // insert would add it permanently).
    let cache_write_start = std::time::Instant::now();
    let entries_count_for_log = entries.len();
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
                created_at: std::time::Instant::now(),
                last_accessed_ms: std::sync::atomic::AtomicU64::new(
                    crate::file_system::listing::caching::epoch_millis_now(),
                ),
            },
        );
    }
    let cache_write_ms = cache_write_start.elapsed().as_millis();

    // Get the volume from VolumeManager to check if it supports watching.
    // Virtual git portal paths (`.git/branches/...` and friends) don't
    // exist on disk, so `notify` would error with "No path was found".
    // Cache invalidation for those listings flows through
    // `git::watcher::invalidate_virtual_listings` instead.
    let watcher_start_t = std::time::Instant::now();
    if !crate::file_system::git::is_virtual(path)
        && volume.supports_watching()
        && let Err(e) = start_watching(listing_id, path)
    {
        log::warn!("Failed to start watcher: {}", e);
        // Continue anyway - watcher is optional enhancement
    }
    let watcher_start_ms = watcher_start_t.elapsed().as_millis();

    // Volume root for the event (the FE uses it to decide "at volume root"). For
    // an archive this is the `.zip` path, so the FE breadcrumb renders
    // `…/foo.zip/inner` and treats the archive root as the volume root.
    let volume_root_path = Some(volume.root().to_path_buf());
    let volume_root = volume_root_path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "/".to_string());

    // Restricted-empty hint: if the listing succeeded but came back empty AT the
    // volume root, and the volume is one we know is commonly hidden by macOS TCC
    // (e.g. iCloud Drive without Full Disk Access), surface a friendly hint instead
    // of a blank pane. Real "I have zero files at the volume root" is rare enough
    // that this hint is acceptable noise, and the FE shows a "Try again" button.
    if total_count == 0
        && volume_root_path.as_deref() == Some(path)
        && let Some(listing_error) = listing_error_for_restricted_empty_root(volume_id, path)
    {
        let message = listing_error.raw_detail.clone();
        crate::mcp::listing_errors::record(listing_id, volume_id, &path.to_string_lossy(), &message);
        events.emit_error(listing_id, message, Some(listing_error));
        return Ok(());
    }

    // Emit completion event
    let emit_t = std::time::Instant::now();
    events.emit_complete(listing_id, total_count, volume_root);
    let to_complete_emit_ms = emit_t.elapsed().as_millis();
    let total_ms = total_start.elapsed().as_millis();

    // Consolidated INFO log for the listing pipeline (Phase 1 instrumentation).
    // Grepable single-line structured record. See stall_probe::listing target.
    log::info!(
        target: "stall_probe::listing",
        "listing_done listing_id={} path={} entries={} read_dir_ms={} sort_ms={} cache_write_ms={} enrich_ms={} watcher_start_ms={} to_complete_emit_ms={} total_ms={}",
        listing_id,
        path.display(),
        entries_count_for_log,
        read_dir_time.as_millis(),
        sort_ms,
        cache_write_ms,
        enrich_ms,
        watcher_start_ms,
        to_complete_emit_ms,
        total_ms,
    );

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

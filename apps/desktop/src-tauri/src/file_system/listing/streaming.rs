//! Streaming directory listing: types, state, and async implementation.
//!
//! Provides non-blocking directory reading with progress events and cancellation.
//! The implementation spawns background tasks and emits Tauri events.

#![allow(dead_code, reason = "ListingProgressEvent is part of public API for future use")]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, LazyLock, RwLock};
use std::time::Duration;

use crate::benchmark;
use crate::file_system::listing::caching::{CachedListing, LISTING_CACHE};
use crate::file_system::listing::sorting::{SortColumn, SortOrder, sort_entries};
use crate::file_system::watcher::start_watching;

// ============================================================================
// Types and state
// ============================================================================

/// Interval for checking cancellation while waiting for directory listing results.
/// This ensures we can respond to ESC within ~100ms even if I/O is blocked.
pub(crate) const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(100);

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
    /// Checked periodically during iteration.
    pub cancelled: AtomicBool,
}

/// Cache for streaming state (separate from completed listings cache)
pub(crate) static STREAMING_STATE: LazyLock<RwLock<HashMap<String, Arc<StreamingListingState>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

// ============================================================================
// Streaming implementation
// ============================================================================

/// Starts a streaming directory listing that returns immediately and emits progress events.
///
/// This is non-blocking - the actual directory reading happens in a background task.
/// Progress is reported via Tauri events every 500ms.
pub async fn list_directory_start_streaming(
    app: tauri::AppHandle,
    volume_id: &str,
    path: &Path,
    include_hidden: bool,
    sort_by: SortColumn,
    sort_order: SortOrder,
    listing_id: String,
) -> Result<StreamingListingStartResult, std::io::Error> {
    // Reset benchmark epoch for this navigation
    benchmark::reset_epoch();
    benchmark::log_event_value("list_directory_start_streaming CALLED", path.display());

    // Create streaming state with cancellation flag
    let state = Arc::new(StreamingListingState {
        cancelled: AtomicBool::new(false),
    });

    // Store state for cancellation
    if let Ok(mut cache) = STREAMING_STATE.write() {
        cache.insert(listing_id.clone(), Arc::clone(&state));
    }

    // Clone values for the spawned task
    let listing_id_for_spawn = listing_id.clone();
    let path_owned = path.to_path_buf();
    let volume_id_owned = volume_id.to_string();
    let app_for_spawn = app.clone();

    // Spawn background task
    tokio::spawn(async move {
        // Clone again for use after spawn_blocking
        let listing_id_for_cleanup = listing_id_for_spawn.clone();
        let app_for_error = app_for_spawn.clone();

        // Run blocking I/O on dedicated thread pool
        let result = tokio::task::spawn_blocking(move || {
            read_directory_with_progress(
                &app_for_spawn,
                &listing_id_for_spawn,
                &state,
                &volume_id_owned,
                &path_owned,
                include_hidden,
                sort_by,
                sort_order,
            )
        })
        .await;

        // Clean up streaming state
        if let Ok(mut cache) = STREAMING_STATE.write() {
            cache.remove(&listing_id_for_cleanup);
        }

        // Handle task result
        use tauri::Emitter;
        match result {
            Err(e) => {
                // Task panicked or was cancelled
                let _ = app_for_error.emit(
                    "listing-error",
                    ListingErrorEvent {
                        listing_id: listing_id_for_cleanup,
                        message: format!("Task failed: {}", e),
                    },
                );
            }
            Ok(Err(e)) => {
                // Function returned an error (e.g., volume not found, permission denied)
                let _ = app_for_error.emit(
                    "listing-error",
                    ListingErrorEvent {
                        listing_id: listing_id_for_cleanup,
                        message: e.to_string(),
                    },
                );
            }
            Ok(Ok(())) => {
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
/// Runs on a blocking thread pool and emits progress events.
/// Uses the Volume abstraction to support both local filesystem and MTP devices.
#[allow(
    clippy::too_many_arguments,
    reason = "Streaming operation requires many state parameters"
)]
fn read_directory_with_progress(
    app: &tauri::AppHandle,
    listing_id: &str,
    state: &Arc<StreamingListingState>,
    volume_id: &str,
    path: &Path,
    include_hidden: bool,
    sort_by: SortColumn,
    sort_order: SortOrder,
) -> Result<(), std::io::Error> {
    use tauri::Emitter;

    benchmark::log_event("read_directory_with_progress START");
    log::debug!(
        "read_directory_with_progress: listing_id={}, volume_id={}, path={}",
        listing_id,
        volume_id,
        path.display()
    );

    // Emit opening event - this is the slow part for network folders
    // (SMB connection establishment, directory handle creation, MTP queries)
    let _ = app.emit(
        "listing-opening",
        ListingOpeningEvent {
            listing_id: listing_id.to_string(),
        },
    );

    // Check cancellation before starting
    if state.cancelled.load(Ordering::Relaxed) {
        benchmark::log_event("read_directory_with_progress CANCELLED (before read)");
        let _ = app.emit(
            "listing-cancelled",
            ListingCancelledEvent {
                listing_id: listing_id.to_string(),
            },
        );
        return Ok(());
    }

    // Get the volume from VolumeManager
    let volume = crate::file_system::get_volume_manager()
        .get(volume_id)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, format!("Volume not found: {}", volume_id)))?;

    // Read directory entries via Volume abstraction
    // Use polling-based cancellation to remain responsive even when filesystem I/O blocks
    // (e.g., on slow/stuck network drives like SMB mounts)
    let read_start = std::time::Instant::now();
    let path_for_thread = path.to_path_buf();
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let result = volume.list_directory(&path_for_thread);
        let _ = tx.send(result);
    });

    // Poll for results, checking cancellation between polls
    let entries_result = loop {
        if state.cancelled.load(Ordering::Relaxed) {
            benchmark::log_event("read_directory_with_progress CANCELLED (during read_dir polling)");
            let _ = app.emit(
                "listing-cancelled",
                ListingCancelledEvent {
                    listing_id: listing_id.to_string(),
                },
            );
            return Ok(());
        }

        match rx.recv_timeout(CANCELLATION_POLL_INTERVAL) {
            Ok(result) => break result,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(std::io::Error::other(
                    "Directory listing thread terminated unexpectedly",
                ));
            }
        }
    };

    let mut entries = entries_result.map_err(|e| std::io::Error::other(e.to_string()))?;
    let read_dir_time = read_start.elapsed();
    benchmark::log_event_value("read_dir COMPLETE, entries", entries.len());

    // Emit read-complete event (before sorting/caching) so UI can show "All N files loaded"
    let _ = app.emit(
        "listing-read-complete",
        ListingReadCompleteEvent {
            listing_id: listing_id.to_string(),
            total_count: entries.len(),
        },
    );

    // Check cancellation one more time before finalizing
    if state.cancelled.load(Ordering::Relaxed) {
        benchmark::log_event("read_directory_with_progress CANCELLED (after read)");
        let _ = app.emit(
            "listing-cancelled",
            ListingCancelledEvent {
                listing_id: listing_id.to_string(),
            },
        );
        return Ok(());
    }

    // Sort entries
    benchmark::log_event("sort START");
    sort_entries(&mut entries, sort_by, sort_order);
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
            let _ = app.emit(
                "listing-cancelled",
                ListingCancelledEvent {
                    listing_id: listing_id.to_string(),
                },
            );
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
    let _ = app.emit(
        "listing-complete",
        ListingCompleteEvent {
            listing_id: listing_id.to_string(),
            total_count,
            max_filename_width,
            volume_root,
        },
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
        benchmark::log_event_value("cancel_listing", listing_id);
    }
}

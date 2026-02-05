//! Streaming directory listing types and state management.

#![allow(dead_code, reason = "ListingProgressEvent is part of public API for future use")]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock, RwLock};
use std::time::Duration;

/// Interval for checking cancellation while waiting for directory listing results.
/// This ensures we can respond to ESC within ~100ms even if I/O is blocked.
pub(super) const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Status of a streaming directory listing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum ListingStatus {
    /// Listing is in progress
    Loading,
    /// Listing completed successfully
    Ready,
    /// Listing was cancelled by the user
    Cancelled,
    /// Listing failed with an error
    Error { message: String },
}

/// Result of starting a streaming directory listing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamingListingStartResult {
    /// Unique listing ID for subsequent API calls
    pub listing_id: String,
    /// Initial status (always "loading")
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
    /// Cancellation flag - checked periodically during iteration
    pub cancelled: AtomicBool,
}

/// Cache for streaming state (separate from completed listings cache)
pub(crate) static STREAMING_STATE: LazyLock<RwLock<HashMap<String, Arc<StreamingListingState>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

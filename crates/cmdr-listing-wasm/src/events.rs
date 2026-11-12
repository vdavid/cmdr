//! The engine's events as the frontend hears them: the same event names and
//! payloads `apps/desktop/src-tauri/src/listing_host/events.rs` emits.
//!
//! The wire structs mirror that file's (they live in the app crate, which can't
//! build for the browser). Kept in sync by `events_test.rs`, which pins each
//! payload's keys against the generated `bindings.ts`.
//!
//! What the desktop does around a listing's end and this doesn't: the archive
//! readings (no archives here), the MCP error ring, the TCC-restricted set, the
//! stale-mount check, and analytics. None has a browser counterpart.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::volume::friendly_error::{
    ListingError, enrich_with_provider, listing_error_for_restricted_empty_root, listing_error_from_volume_error,
};
use cmdr_fs::volume::{ListingPhase, ListingProgress, VolumeError};
use cmdr_listing::host::{ListingEnd, ListingEvents};
use cmdr_listing::{DirectoryDiffBatch, ListingId, SortColumn, StalledOn};
use serde::Serialize;

/// Where events go: the page's callback in the browser, a collector in tests.
pub trait EventSink: Send + Sync {
    /// One event: its Tauri name (`"listing-complete"`) and its JSON payload.
    fn emit(&self, event: &str, payload: serde_json::Value);

    /// One event for one window only, as Tauri's `emit_to(label)` sends it (the viewer's
    /// `viewer-media-load`). A sink with no windows hears it as any other event.
    fn emit_to(&self, window_label: &str, event: &str, payload: serde_json::Value) {
        let _ = window_label;
        self.emit(event, payload);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ListingIdEvent<'a> {
    pub(crate) listing_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ListingProgressEvent<'a> {
    pub(crate) listing_id: &'a str,
    pub(crate) loaded_count: usize,
    pub(crate) phase: Option<ListingPhase>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ListingStalledEvent<'a> {
    pub(crate) listing_id: &'a str,
    pub(crate) stalled_on: StalledOn,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ListingReadCompleteEvent<'a> {
    pub(crate) listing_id: &'a str,
    pub(crate) total_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ListingCompleteEvent<'a> {
    pub(crate) listing_id: &'a str,
    pub(crate) total_count: usize,
    pub(crate) hidden_count: usize,
    pub(crate) volume_root: String,
    pub(crate) stored_path: Option<String>,
    pub(crate) effective_sort_by: SortColumn,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ListingErrorEvent<'a> {
    pub(crate) listing_id: &'a str,
    pub(crate) message: String,
    pub(crate) error: Option<ListingError>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DirectoryDiff<'a> {
    pub(crate) listing_id: &'a str,
    pub(crate) batches: Vec<DirectoryDiffBatch>,
}

/// `listing-respelled` and `directory-deleted` share this shape.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ListingPathEvent<'a> {
    pub(crate) listing_id: &'a str,
    pub(crate) path: String,
}

/// The browser's `ListingEvents`: facts in, wire events out.
pub(crate) struct DemoEvents {
    sink: Arc<dyn EventSink>,
}

impl DemoEvents {
    pub(crate) fn new(sink: Arc<dyn EventSink>) -> Self {
        Self { sink }
    }

    pub(crate) fn send(&self, event: &str, payload: impl Serialize) {
        match serde_json::to_value(payload) {
            Ok(payload) => self.sink.emit(event, payload),
            Err(e) => log::warn!(target: "listing", "couldn't serialize `{event}`: {e}"),
        }
    }

    fn failed(&self, listing_id: &str, path: &Path, error: &VolumeError) {
        let mut listing_error = listing_error_from_volume_error(error, path);
        enrich_with_provider(&mut listing_error, path);
        self.send(
            "listing-error",
            ListingErrorEvent {
                listing_id,
                message: error.to_string(),
                error: Some(listing_error),
            },
        );
    }
}

impl ListingEvents for DemoEvents {
    fn opening(&self, id: &ListingId) {
        self.send(
            "listing-opening",
            ListingIdEvent {
                listing_id: id.as_str(),
            },
        );
    }

    fn stalled(&self, id: &ListingId, on: StalledOn) {
        let listing_id = id.as_str();
        self.send(
            "listing-stalled",
            ListingStalledEvent {
                listing_id,
                stalled_on: on,
            },
        );
    }

    fn progress(&self, id: &ListingId, progress: ListingProgress) {
        self.send(
            "listing-progress",
            ListingProgressEvent {
                listing_id: id.as_str(),
                loaded_count: progress.entries(),
                phase: progress.phase,
            },
        );
    }

    fn read_complete(&self, id: &ListingId, total: usize) {
        let listing_id = id.as_str();
        self.send(
            "listing-read-complete",
            ListingReadCompleteEvent {
                listing_id,
                total_count: total,
            },
        );
    }

    fn finished(&self, id: &ListingId, end: ListingEnd) {
        let listing_id = id.as_str();
        match end {
            ListingEnd::Complete {
                volume_id,
                path,
                total_count,
                hidden_count,
                volume_root,
                stored_path,
                empty_at_volume_root,
                effective_sort_by,
            } => {
                let listed_path = stored_path.as_deref().unwrap_or(&path);
                if empty_at_volume_root
                    && let Some(listing_error) = listing_error_for_restricted_empty_root(&volume_id, listed_path)
                {
                    let message = listing_error.raw_detail.clone();
                    let error = Some(listing_error);
                    self.send(
                        "listing-error",
                        ListingErrorEvent {
                            listing_id,
                            message,
                            error,
                        },
                    );
                    return;
                }
                self.send(
                    "listing-complete",
                    ListingCompleteEvent {
                        listing_id,
                        total_count,
                        hidden_count,
                        volume_root: volume_root.to_string_lossy().into_owned(),
                        stored_path: stored_path.map(|p| p.to_string_lossy().into_owned()),
                        effective_sort_by,
                    },
                );
            }
            ListingEnd::Cancelled { .. } => self.send("listing-cancelled", ListingIdEvent { listing_id }),
            ListingEnd::Failed { path, error, .. } => self.failed(listing_id, &path, &error),
        }
    }

    fn diff(&self, id: &ListingId, batches: Vec<DirectoryDiffBatch>) {
        self.send(
            "directory-diff",
            DirectoryDiff {
                listing_id: id.as_str(),
                batches,
            },
        );
    }

    fn respelled(&self, id: &ListingId, path: &Path) {
        let path = path.to_string_lossy().into_owned();
        self.send(
            "listing-respelled",
            ListingPathEvent {
                listing_id: id.as_str(),
                path,
            },
        );
    }
}

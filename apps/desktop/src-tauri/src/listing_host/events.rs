//! The listing engine's events, as this app's frontend hears them.
//!
//! The engine reports facts through `ListingEvents` (`cmdr_listing::host`);
//! [`TauriListingEvents`] is this app's answer, and the only place those facts
//! become wire events. It owns every policy on them: the typed `listing-error`
//! classification (with the damaged-archive, password, and cloud-provider
//! readings), the TCC restricted-empty-root hint, the PII-free analytics
//! category, and the bookkeeping around a listing's end (the MCP recent-errors
//! ring, the TCC-restricted set, the stale-mount check). The wire structs below
//! keep their names and derives, so the generated bindings don't move.
//!
//! Split in two so a test drives the whole policy without an `AppHandle`: the
//! payloads go out through a [`ListingWire`] (`AppHandle` in the app, a collector
//! in tests), the bookkeeping through [`ListingEndEffects`].

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri_specta::Event;

use cmdr_fs::volume::friendly_error::{
    ListingError, archive_needs_password_listing_error, archive_unreadable_listing_error, enrich_with_provider,
    listing_error_for_restricted_empty_root, listing_error_from_volume_error,
};
use cmdr_fs::volume::{ListingPhase, ListingProgress, VolumeError};
use cmdr_listing::ListingId;
use cmdr_listing::StalledOn;
use cmdr_listing::host::{ListingEnd, ListingEvents};
use cmdr_listing::{DirectoryDiffBatch, SortColumn};

// ============================================================================
// Wire payloads
// ============================================================================

/// Progress event payload
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "listing-progress")]
pub struct ListingProgressEvent {
    pub listing_id: String,
    pub loaded_count: usize,
    /// What the listing is doing before its first entry, when it has its own
    /// measure (a backup's index load); `None` while it only enumerates. Typed,
    /// so the frontend words it.
    pub phase: Option<ListingPhase>,
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
    /// The directory in its volume's own spelling, when that differs from the
    /// path the listing was asked for: a foreign path (typed, restored, carried
    /// over from the kernel mount) that a byte-exact volume stores under another
    /// spelling (`foreign_path.rs`). The pane adopts it, so its tab, history, and
    /// every child path carry the stored bytes. `None` for a path that listed as
    /// given, which is nearly every listing.
    pub stored_path: Option<String>,
    /// The column the rows are ordered by: the requested one, or Modified when
    /// it's a date this volume doesn't record. The pane keeps its requested
    /// column; this only says which date its rows follow.
    pub effective_sort_by: SortColumn,
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

/// Stalled event payload: the read has gone `StallPolicy::stall_after` without
/// a new entry. The listing keeps waiting and retrying (`stall.rs`); a later
/// progress, complete, error, or cancelled event for the same id supersedes it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "listing-stalled")]
pub struct ListingStalledEvent {
    pub listing_id: String,
    /// What the folder lives on, which picks the screen's wording (`stalled_on.rs`).
    pub stalled_on: StalledOn,
}

/// `directory-diff` event sent to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryDiff {
    pub listing_id: String,
    pub batches: Vec<DirectoryDiffBatch>,
}

/// `listing-respelled`: an open listing now shows its directory under another
/// spelling of the same path, which the pane adopts as its own.
///
/// Sent after a backend swap re-reads a listing (`foreign_path.rs`'s
/// `respell_listings_on_volume`); a listing that lands on a stored spelling when
/// it's first opened says so in `listing-complete` instead.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "listing-respelled")]
pub struct ListingRespelledEvent {
    pub listing_id: String,
    /// The directory in its volume's own spelling.
    pub path: String,
}

// ============================================================================
// Where the payloads go
// ============================================================================

/// Sends the listing wire events. Production: the `AppHandle`. Tests: a
/// collector that keeps them for assertion.
pub(crate) trait ListingWire: Send + Sync {
    fn emit_opening(&self, listing_id: &str);
    fn emit_stalled(&self, listing_id: &str, stalled_on: StalledOn);
    fn emit_progress(&self, listing_id: &str, loaded_count: usize, phase: Option<ListingPhase>);
    fn emit_read_complete(&self, listing_id: &str, total_count: usize);
    fn emit_complete(&self, event: ListingCompleteEvent);
    fn emit_error(&self, listing_id: &str, message: String, error: Option<ListingError>);
    fn emit_cancelled(&self, listing_id: &str);
    fn emit_diff(&self, listing_id: &str, batches: Vec<DirectoryDiffBatch>);
    fn emit_respelled(&self, listing_id: &str, path: &Path);
}

impl ListingWire for tauri::AppHandle {
    fn emit_opening(&self, listing_id: &str) {
        let _ = ListingOpeningEvent {
            listing_id: listing_id.to_string(),
        }
        .emit(self);
    }

    fn emit_stalled(&self, listing_id: &str, stalled_on: StalledOn) {
        let _ = ListingStalledEvent {
            listing_id: listing_id.to_string(),
            stalled_on,
        }
        .emit(self);
    }

    fn emit_progress(&self, listing_id: &str, loaded_count: usize, phase: Option<ListingPhase>) {
        let _ = ListingProgressEvent {
            listing_id: listing_id.to_string(),
            loaded_count,
            phase,
        }
        .emit(self);
    }

    fn emit_read_complete(&self, listing_id: &str, total_count: usize) {
        let _ = ListingReadCompleteEvent {
            listing_id: listing_id.to_string(),
            total_count,
        }
        .emit(self);
    }

    fn emit_complete(&self, event: ListingCompleteEvent) {
        let _ = event.emit(self);
    }

    fn emit_error(&self, listing_id: &str, message: String, error: Option<ListingError>) {
        // PII-free analytics: a listing failed with a categorized error. Only the
        // category enum crosses; never the path, message, or any provider detail.
        if let Some(category) = error_analytics_category(error.as_ref()) {
            crate::analytics::events::capture("error_encountered", serde_json::json!({ "category": category }));
        }
        let _ = ListingErrorEvent {
            listing_id: listing_id.to_string(),
            message,
            error,
        }
        .emit(self);
    }

    fn emit_cancelled(&self, listing_id: &str) {
        let _ = ListingCancelledEvent {
            listing_id: listing_id.to_string(),
        }
        .emit(self);
    }

    fn emit_diff(&self, listing_id: &str, batches: Vec<DirectoryDiffBatch>) {
        let diff = DirectoryDiff {
            listing_id: listing_id.to_string(),
            batches,
        };
        if let Err(e) = diff.emit(self) {
            log::warn!("diff_emitter: couldn't emit batched event: {}", e);
        }
    }

    fn emit_respelled(&self, listing_id: &str, path: &Path) {
        let event = ListingRespelledEvent {
            listing_id: listing_id.to_string(),
            path: path.to_string_lossy().into_owned(),
        };
        if let Err(e) = event.emit(self) {
            log::warn!("respell_listing: couldn't emit listing-respelled: {e}");
        }
    }
}

/// The analytics category a `listing-error` reports: the classification's
/// category enum as its wire string, and nothing else (no path, message, or
/// provider). `None` for an unclassified error, which reports nothing.
pub(crate) fn error_analytics_category(error: Option<&ListingError>) -> Option<String> {
    let error = error?;
    serde_json::to_value(error.category)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
}

// ============================================================================
// The bookkeeping around a listing's end
// ============================================================================

/// What a streaming listing's end does outside its own events. Production
/// answers with [`AppListingEndEffects`]; a test records the calls.
pub(crate) trait ListingEndEffects: Send + Sync {
    /// Records a failed listing into the MCP recent-errors ring.
    fn record_recent_error(&self, listing_id: &str, volume_id: &str, path: &Path, message: &str, error: &ListingError);
    /// Records a `PermissionDenied` path as possibly TCC-restricted.
    fn record_denial(&self, path: &Path);
    /// Drops `path` from the TCC-restricted set.
    fn clear_restricted(&self, path: &Path);
    /// Hands a listing's error to the volume's stale-mount check.
    fn note_root_failure(&self, volume_id: &str, error: &VolumeError);
}

/// The app's answers to [`ListingEndEffects`].
pub(crate) struct AppListingEndEffects;

impl ListingEndEffects for AppListingEndEffects {
    fn record_recent_error(&self, listing_id: &str, volume_id: &str, path: &Path, message: &str, error: &ListingError) {
        crate::ui_state::listing_errors::record(listing_id, volume_id, &path.to_string_lossy(), message, Some(error));
    }

    fn record_denial(&self, path: &Path) {
        crate::restricted_paths::record_denial(path);
    }

    fn clear_restricted(&self, path: &Path) {
        crate::restricted_paths::clear_path(path);
    }

    fn note_root_failure(&self, volume_id: &str, error: &VolumeError) {
        crate::file_system::volume::note_root_failure(volume_id, error);
    }
}

// ============================================================================
// The adapter
// ============================================================================

/// This app's `ListingEvents`: the engine's facts, turned into wire events and
/// the bookkeeping around them.
pub(crate) struct TauriListingEvents<W = tauri::AppHandle, E = AppListingEndEffects> {
    wire: W,
    effects: E,
}

impl TauriListingEvents {
    /// The app's wiring: events to the frontend, bookkeeping into the app.
    pub(crate) fn new(app: tauri::AppHandle) -> Self {
        Self::with(app, AppListingEndEffects)
    }
}

impl<W: ListingWire, E: ListingEndEffects> TauriListingEvents<W, E> {
    /// The same policy over another wire and other bookkeeping (tests).
    pub(crate) fn with(wire: W, effects: E) -> Self {
        Self { wire, effects }
    }

    /// A listing that landed in the cache: `listing-complete`, or the
    /// restricted-empty-root hint in its place.
    fn complete(&self, listing_id: &str, end: Completed<'_>) {
        // Restricted-empty hint: if the listing succeeded but came back empty AT the
        // volume root, and the volume is one we know is commonly hidden by macOS TCC
        // (e.g. iCloud Drive without Full Disk Access), surface a friendly hint instead
        // of a blank pane. Real "I have zero files at the volume root" is rare enough
        // that this hint is acceptable noise, and the FE shows a "Try again" button.
        if end.empty_at_volume_root
            && let Some(listing_error) = listing_error_for_restricted_empty_root(end.volume_id, end.listed_path)
        {
            let message = listing_error.raw_detail.clone();
            self.effects
                .record_recent_error(listing_id, end.volume_id, end.listed_path, &message, &listing_error);
            self.wire.emit_error(listing_id, message, Some(listing_error));
            return;
        }
        self.wire.emit_complete(ListingCompleteEvent {
            listing_id: listing_id.to_string(),
            total_count: end.total_count,
            volume_root: end.volume_root.to_string_lossy().to_string(),
            stored_path: end.stored_path.map(|path| path.to_string_lossy().into_owned()),
            effective_sort_by: end.effective_sort_by,
        });
    }

    /// A failed read: classified into a `listing-error`, with the app bookkeeping
    /// around it.
    fn failed(&self, listing_id: &str, volume_id: &str, path: &Path, e: VolumeError) {
        // A failed `.zip` browse — the archive is damaged, encrypted, an
        // unsupported format, or a file with an archive extension that isn't
        // really an archive — collapses to `NotSupported`/`IoError` at the
        // `ArchiveVolume` layer. Swap in the dedicated "damaged archive" copy
        // so the pane doesn't show a generic listing error. A valid archive
        // with a missing inner path stays `NotFound` (not an integrity fault),
        // and the archive case skips provider enrichment (no cloud suggestion).
        let listing_error = if let VolumeError::NeedsPassword { wrong_attempt } = e {
            // A HEADER-encrypted archive needs its password to even list; the
            // FE renders a password prompt, not an error pane. Skip provider
            // enrichment (no cloud suggestion on a password prompt).
            archive_needs_password_listing_error(wrong_attempt)
        } else if matches!(e, VolumeError::NotSupported | VolumeError::IoError { .. })
            && cmdr_archive::path_targets_archive_file(path)
        {
            archive_unreadable_listing_error()
        } else {
            let mut generic = listing_error_from_volume_error(&e, path);
            enrich_with_provider(&mut generic, path);
            generic
        };
        if matches!(&e, VolumeError::PermissionDenied { .. }) {
            self.effects.record_denial(path);
        }
        // A stale-mount errno here is the only evidence we ever get that
        // the volume's active mount died: probing for it would block on
        // the wedged mount itself. If the same filesystem is reachable
        // through another mount, this moves the volume there.
        self.effects.note_root_failure(volume_id, &e);
        let message = e.to_string();
        // Record into the MCP recent-errors ring buffer so `cmdr://state`
        // surfaces what just failed, without callers grepping the log file.
        self.effects
            .record_recent_error(listing_id, volume_id, path, &message, &listing_error);
        // One line per failed listing, carrying the TYPED reason, so a log
        // answers "why did this listing fail" without reading code.
        log::debug!(
            "listing {listing_id} on volume {volume_id} ended in error: {:?} ({:?}){}",
            listing_error.reason,
            listing_error.category,
            e.errno_field(),
        );
        self.wire.emit_error(listing_id, message, Some(listing_error));
    }
}

/// The parts of a [`ListingEnd::Complete`] the wire needs.
struct Completed<'a> {
    volume_id: &'a str,
    /// The directory as listed: the stored spelling when there was one.
    listed_path: &'a Path,
    total_count: usize,
    volume_root: &'a Path,
    stored_path: Option<PathBuf>,
    empty_at_volume_root: bool,
    effective_sort_by: SortColumn,
}

impl<W: ListingWire, E: ListingEndEffects> ListingEvents for TauriListingEvents<W, E> {
    fn opening(&self, id: &ListingId) {
        self.wire.emit_opening(id.as_str());
    }

    fn stalled(&self, id: &ListingId, on: StalledOn) {
        self.wire.emit_stalled(id.as_str(), on);
    }

    fn progress(&self, id: &ListingId, progress: ListingProgress) {
        self.wire.emit_progress(id.as_str(), progress.entries(), progress.phase);
    }

    fn read_complete(&self, id: &ListingId, total: usize) {
        self.wire.emit_read_complete(id.as_str(), total);
    }

    fn finished(&self, id: &ListingId, end: ListingEnd) {
        let listing_id = id.as_str();
        match end {
            ListingEnd::Complete {
                volume_id,
                path,
                total_count,
                volume_root,
                stored_path,
                empty_at_volume_root,
                effective_sort_by,
            } => {
                let listed_path = stored_path.as_deref().unwrap_or(&path).to_path_buf();
                self.complete(
                    listing_id,
                    Completed {
                        volume_id: &volume_id,
                        listed_path: &listed_path,
                        total_count,
                        volume_root: &volume_root,
                        stored_path,
                        empty_at_volume_root,
                        effective_sort_by,
                    },
                );
                // Success: if this path was previously recorded as TCC-restricted,
                // drop it: the user must have granted access (per-folder TCC popup
                // or FDA toggle). The restricted-empty root lands here too.
                self.effects.clear_restricted(&path);
            }
            ListingEnd::Cancelled { path, .. } => {
                self.wire.emit_cancelled(listing_id);
                // A cancel clears the path from the restricted set exactly as a
                // success does.
                self.effects.clear_restricted(&path);
            }
            ListingEnd::Failed { volume_id, path, error } => self.failed(listing_id, &volume_id, &path, error),
        }
    }

    fn diff(&self, id: &ListingId, batches: Vec<DirectoryDiffBatch>) {
        self.wire.emit_diff(id.as_str(), batches);
    }

    fn respelled(&self, id: &ListingId, path: &Path) {
        self.wire.emit_respelled(id.as_str(), path);
    }
}

/// Keeps every wire event for assertion: the frontend's view of a listing,
/// without an `AppHandle`.
#[cfg(test)]
// DEFAULT-OK: a collector that has seen no event yet holds empty lists.
#[derive(Default)]
pub(crate) struct WireCollector {
    pub errors: std::sync::Mutex<Vec<(String, String)>>,
    /// Per error, the typed classification it carried.
    pub classified: std::sync::Mutex<Vec<(String, Option<ListingError>)>>,
    /// Per progress event, its loaded count and phase.
    pub progress: std::sync::Mutex<Vec<(usize, Option<ListingPhase>)>>,
    /// Every event's kind (`"opening"`, `"error"`, …), in emit order.
    pub sequence: std::sync::Mutex<Vec<&'static str>>,
}

#[cfg(test)]
impl WireCollector {
    /// The kinds of every event so far, in emit order.
    pub(crate) fn sequence(&self) -> Vec<&'static str> {
        use cmdr_fs::ignore_poison::IgnorePoison;
        self.sequence.lock_ignore_poison().clone()
    }

    fn note(&self, kind: &'static str) {
        use cmdr_fs::ignore_poison::IgnorePoison;
        self.sequence.lock_ignore_poison().push(kind);
    }
}

#[cfg(test)]
impl ListingWire for std::sync::Arc<WireCollector> {
    fn emit_opening(&self, _listing_id: &str) {
        self.note("opening");
    }
    fn emit_stalled(&self, _listing_id: &str, _stalled_on: StalledOn) {
        self.note("stalled");
    }
    fn emit_progress(&self, _listing_id: &str, loaded_count: usize, phase: Option<ListingPhase>) {
        use cmdr_fs::ignore_poison::IgnorePoison;
        self.note("progress");
        self.progress.lock_ignore_poison().push((loaded_count, phase));
    }
    fn emit_read_complete(&self, _listing_id: &str, _total_count: usize) {
        self.note("read_complete");
    }
    fn emit_complete(&self, _event: ListingCompleteEvent) {
        self.note("complete");
    }
    fn emit_error(&self, listing_id: &str, message: String, error: Option<ListingError>) {
        use cmdr_fs::ignore_poison::IgnorePoison;
        self.note("error");
        self.errors.lock_ignore_poison().push((listing_id.to_string(), message));
        self.classified
            .lock_ignore_poison()
            .push((listing_id.to_string(), error));
    }
    fn emit_cancelled(&self, _listing_id: &str) {
        self.note("cancelled");
    }
    fn emit_diff(&self, _listing_id: &str, _batches: Vec<DirectoryDiffBatch>) {
        self.note("diff");
    }
    fn emit_respelled(&self, _listing_id: &str, _path: &Path) {
        self.note("respelled");
    }
}

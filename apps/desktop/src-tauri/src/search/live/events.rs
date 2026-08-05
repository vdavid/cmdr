//! What a live search reports while it runs: the event family, and the sink it
//! reports through.
//!
//! Shaped after `file_system/listing/streaming.rs` — a `-progress` stream, one
//! terminal event of three, and a sink trait so the run itself never touches
//! Tauri. The difference is the **run id**: a search dialog supersedes its own
//! query as the user types, and every event carries the run it belongs to so the
//! frontend can drop the ones that belong to a query it has moved on from
//! (`docs/specs/unindexed-search-plan.md` Decision 11). ❌ The run id is not a
//! cancellation: a superseded run's WALK keeps going, because walking is coverage
//! work and matching is query work.

use serde::{Deserialize, Serialize};
use tauri_specta::Event;

use crate::search::types::SearchResultEntry;

/// Which part of a live search produced an event.
///
/// Three honest waits rather than one spinner: resolving coverage can mean a
/// multi-second arena load, reading the index is fast, and the walk is unbounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SearchPhase {
    /// Asking the index what it can't answer for yet, and loading its arena.
    ResolvingCoverage,
    /// Scanning the arena: the half the index already covers.
    ReadingIndex,
    /// Walking the ground the index doesn't cover, live.
    Walking,
}

/// How a live search's walk ended. Typed, because three of the four leave the
/// result list INCOMPLETE and the copy differs
/// (`.claude/rules/no-string-matching.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum WalkEnding {
    /// The index already covered the whole scope, so nothing had to be walked.
    NothingToWalk,
    /// Every frontier root this run took was covered end to end.
    Completed,
    /// It stopped before covering its frontier: the drive went away mid-walk, a
    /// root couldn't be read, or the volume couldn't be walked at all. Whatever
    /// it did read is in the index; the rest is still frontier and the next
    /// search asks for it again.
    Interrupted,
    /// Somebody stopped it (Escape, or the dialog closing).
    Cancelled,
}

/// What a live run could NOT answer for, gathered in one place so a terminal
/// event says it once.
///
/// `Run` in the name because the operation log has a `SearchCoverage` of its own
/// (how much of a copy's source tree a journal search covered), and two types of
/// one name can't both cross specta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchRunCoverage {
    /// How the walk ended. Anything but [`WalkEnding::Completed`] /
    /// [`WalkEnding::NothingToWalk`] means the list is a lower bound.
    pub walk: WalkEnding,
    /// Directories nothing is going to walk, as absolute paths: either a walk
    /// tried and can't read one (permission denied), or it won't read one at all
    /// (a NAS snapshot tree, whose per-snapshot copies the scanner refuses on
    /// purpose).
    ///
    /// Two causes, ONE list, and nothing here tells them apart — so the UI states
    /// the fact and names both possibilities rather than guessing
    /// (`lib/search/DETAILS.md` § The live search). ❌ Don't write copy that
    /// claims it's one of them. Add a typed cause here if a caller ever needs to
    /// ACT on one (M8's Full Disk Access route is the candidate).
    pub unreadable: Vec<String>,
    /// Ground another walk on this volume is covering right now, so this run left
    /// it alone. Its rows reach the same index, so this is "these arrive a bit
    /// later", never "these are lost".
    pub still_covering: Vec<String>,
    /// Scope paths that routed to this volume but aren't in its index and
    /// couldn't be walked either. The typed "Cmdr can't speak for this folder"
    /// signal; ❌ never worded as "that folder doesn't exist".
    pub unresolved_scopes: Vec<String>,
    /// Whether the result cap was reached. The walk carries on past it (the count
    /// keeps rising), only the rows stop.
    pub capped: bool,
    /// The ONE volume this run covered, as routing resolved it.
    pub target_volume_id: String,
}

/// A batch of results, plus where the run has got to.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "search-progress")]
pub struct SearchProgressEvent {
    /// The run these results belong to. Drop anything naming a run you've
    /// superseded.
    pub run_id: String,
    pub phase: SearchPhase,
    /// Rows found since the last event, in arrival order. Empty on a
    /// progress-only event (a walk grinding through folders that match nothing,
    /// or a count-only search).
    pub entries: Vec<SearchResultEntry>,
    /// Matches so far, counting the ones past the result cap. "N so far" while
    /// walking, exact once the terminal event lands.
    pub match_count: u32,
    /// Directories the walk has turned up so far. FOUND, not finished: a
    /// directory is counted when the walk discovers it, some way before it reads
    /// what's inside. Progress with no denominator, on purpose — the total is
    /// unknown by definition, and a fabricated percentage or ETA would be a lie
    /// (Decision 14).
    pub dirs_found: u64,
    /// Where the walk was as of this batch. Indicative, not a cursor: the local
    /// walker reads many directories at once.
    pub current_path: Option<String>,
    /// Whether the cap is reached, so no further rows will arrive.
    pub capped: bool,
}

/// The run finished on its own terms — which is not the same as "the answer is
/// complete"; [`SearchRunCoverage::walk`] says which.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "search-complete")]
pub struct SearchCompleteEvent {
    pub run_id: String,
    /// The exact total for everything this run covered.
    pub match_count: u32,
    pub coverage: SearchRunCoverage,
}

/// Somebody stopped the run. Its results stay on screen: everything already
/// found is real, and everything the walk read is in the index for next time.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "search-cancelled")]
pub struct SearchCancelledEvent {
    pub run_id: String,
    pub match_count: u32,
    pub coverage: SearchRunCoverage,
}

/// Why a run couldn't answer at all. Typed for the branch, with the sentence
/// alongside for display (the bare-message contract search results already have).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SearchRunError {
    /// The query can't be run: an invalid pattern, or one that narrows nothing
    /// while ground still needs walking (a walk over an unknown filesystem can't
    /// afford "show me everything").
    Query,
    /// The volume's index file exists but won't open or read. Distinct from
    /// "never indexed", which is not an error — that volume gets walked.
    IndexUnreadable,
}

/// The run couldn't run.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "search-error")]
pub struct SearchErrorEvent {
    pub run_id: String,
    /// Typed, word-free classification. Branch on this.
    pub error: SearchRunError,
    /// The sentence to show. Rendered backend-side for the same reason the
    /// engine's "Query too broad" is.
    pub message: String,
}

/// Where a live search's events go.
///
/// Production emits Tauri events; tests collect them. Same reason
/// `ListingEventSink` exists: the run's logic is worth testing without a webview
/// in the room.
pub(crate) trait SearchEventSink: Send + Sync {
    fn emit_progress(&self, event: SearchProgressEvent);
    fn emit_complete(&self, event: SearchCompleteEvent);
    fn emit_cancelled(&self, event: SearchCancelledEvent);
    fn emit_error(&self, event: SearchErrorEvent);
}

/// The production sink: `app.emit()` per event.
pub(crate) struct TauriSearchEventSink {
    app: tauri::AppHandle,
}

impl TauriSearchEventSink {
    pub(crate) fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

/// The test sink: keeps every event for inspection.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct CollectorSearchEventSink {
    pub progress: std::sync::Mutex<Vec<SearchProgressEvent>>,
    pub complete: std::sync::Mutex<Vec<SearchCompleteEvent>>,
    pub cancelled: std::sync::Mutex<Vec<SearchCancelledEvent>>,
    pub errors: std::sync::Mutex<Vec<SearchErrorEvent>>,
}

#[cfg(test)]
impl CollectorSearchEventSink {
    /// Every result row emitted, in the order it was emitted.
    pub(crate) fn rows(&self) -> Vec<SearchResultEntry> {
        use crate::ignore_poison::IgnorePoison;

        self.progress
            .lock_ignore_poison()
            .iter()
            .flat_map(|event| event.entries.clone())
            .collect()
    }

    /// How many rows each progress event carried, empties included.
    pub(crate) fn batch_sizes(&self) -> Vec<usize> {
        use crate::ignore_poison::IgnorePoison;

        self.progress
            .lock_ignore_poison()
            .iter()
            .map(|event| event.entries.len())
            .collect()
    }
}

#[cfg(test)]
impl SearchEventSink for CollectorSearchEventSink {
    fn emit_progress(&self, event: SearchProgressEvent) {
        use crate::ignore_poison::IgnorePoison;
        self.progress.lock_ignore_poison().push(event);
    }

    fn emit_complete(&self, event: SearchCompleteEvent) {
        use crate::ignore_poison::IgnorePoison;
        self.complete.lock_ignore_poison().push(event);
    }

    fn emit_cancelled(&self, event: SearchCancelledEvent) {
        use crate::ignore_poison::IgnorePoison;
        self.cancelled.lock_ignore_poison().push(event);
    }

    fn emit_error(&self, event: SearchErrorEvent) {
        use crate::ignore_poison::IgnorePoison;
        self.errors.lock_ignore_poison().push(event);
    }
}

impl SearchEventSink for TauriSearchEventSink {
    fn emit_progress(&self, event: SearchProgressEvent) {
        let _ = event.emit(&self.app);
    }

    fn emit_complete(&self, event: SearchCompleteEvent) {
        let _ = event.emit(&self.app);
    }

    fn emit_cancelled(&self, event: SearchCancelledEvent) {
        let _ = event.emit(&self.app);
    }

    fn emit_error(&self, event: SearchErrorEvent) {
        let _ = event.emit(&self.app);
    }
}

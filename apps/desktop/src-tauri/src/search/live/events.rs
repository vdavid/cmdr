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
/// result list INCOMPLETE and the copy differs.
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

/// What ground a run's answer was drawn from: the index, a live walk, or both.
///
/// Derived from the coverage question rather than from how the walk ended, so it
/// says what the search HAD to do, not how far it got — a cancelled run over
/// half-covered ground is still `Mixed`, with [`WalkEnding`] saying the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CoverageKind {
    /// The index covered the whole scope. Nothing had to be walked.
    Covered,
    /// Nothing in the scope was covered, so every bit of the answer was walked
    /// live. The cold-drive case this effort exists for.
    Live,
    /// Part of the scope came from the index and part from the walk.
    Mixed,
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
    /// Which ground this run's answer came from. The measure of how often a
    /// search still needs to walk at all, and the one field here that describes
    /// the QUESTION rather than what came back from it.
    pub kind: CoverageKind,
    /// Directories a walk tried to read and was REFUSED, as absolute paths.
    ///
    /// The half a user can act on: on macOS this is usually Full Disk Access, and
    /// granting it heals the mark on the next search (the successful listing
    /// clears it). ❌ Keep it apart from [`declined`](Self::declined) — offering
    /// Full Disk Access over a snapshot folder is advice that does nothing.
    pub permission_denied: Vec<String>,
    /// Directories no walk will read at all, by Cmdr's own choice: a NAS snapshot
    /// tree, whose per-snapshot hardlinked copies the scanner refuses on purpose
    /// (44 TB reported on a 10 TB volume).
    ///
    /// Nothing for the user to fix, so the copy explains rather than offers.
    pub declined: Vec<String>,
    /// Ground another walk on this volume is covering right now, so this run left
    /// it alone. Its rows reach the same index, so this is "these arrive a bit
    /// later", never "these are lost".
    pub still_covering: Vec<String>,
    /// Scope paths that routed to this volume but aren't in its index and
    /// couldn't be walked either. The typed "Cmdr can't speak for this folder"
    /// signal; ❌ never worded as "that folder doesn't exist".
    pub unresolved_scopes: Vec<String>,
    /// Whether ground was given up on: a directory that stopped responding, one
    /// that failed with an errno the walk can't act on, or a subtree pruned by the
    /// walker's consecutive-failure budget. Covers both what THIS run's walk gave
    /// up on and what an earlier walk recorded as `UnreadableCause::Abandoned` (the
    /// reason the frontier didn't offer it to this run at all).
    ///
    /// TRUE means the list is a lower bound even when [`walk`](Self::walk) is
    /// [`WalkEnding::Completed`] — the third way a run can be short, alongside
    /// cancel and disconnect (Accepted difference 9), and the quiet one: nothing
    /// else on the wire hints at it. ❌ Don't fold it into `walk`: Cmdr retries
    /// this ground on a backoff, which is a different sentence from "the drive went
    /// away".
    pub abandoned_ground: bool,
    /// How many PLACES the walk gave up on, for a note that can say how much of
    /// the drive this is about instead of leaving the reader to imagine it.
    ///
    /// Folders grouped by their parent, ❌ never the raw folder count: a wedged
    /// mount marked 1,497 directories on one real machine, which
    /// `coverage_for_scope` already cuts to 76 shallowest ancestors, and grouping
    /// those lands on the one place the user would recognize. Reporting 1,497
    /// would be true and useless.
    ///
    /// `0` with [`abandoned_ground`](Self::abandoned_ground) true is a real state
    /// and the note handles it: this run's own walk gave up on ground it never
    /// recorded a path for, so something was missed and nothing can say where.
    #[serde(default)]
    pub abandoned_locations: u32,
    /// Whether the result cap was reached. The walk carries on past it (the count
    /// keeps rising), only the rows stop.
    pub capped: bool,
    /// The ONE volume this run covered, as routing resolved it.
    pub target_volume_id: String,
    /// How many matches an exclusion rule kept out of the count: the
    /// system/build/cache tier (on unless the query turns it off) plus any `!`
    /// excludes in the scope.
    ///
    /// Coverage, not a statistic: the defaults are right for "find my invoice"
    /// and exactly wrong for "where is my disk space going", where the caches ARE
    /// the answer. A caller that can't see this number reads a filtered count as
    /// the whole truth. Counted across BOTH halves — the arena scan and the walk
    /// — so a live run doesn't under-report it.
    #[serde(default)]
    pub hidden_by_excludes: u32,
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

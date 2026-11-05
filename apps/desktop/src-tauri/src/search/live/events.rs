//! What a live search reports while it runs: the event family, and the sinks that
//! put `cmdr_search::run`'s facts on it. The payload data (`SearchRunCoverage`,
//! `SearchPhase`, `SearchRunError`, ...) is the crate's; the envelopes are the
//! app's.
//!
//! Shaped after `crates/cmdr-listing/src/streaming.rs`: a `-progress` stream, one
//! terminal event of three, and a sink trait (`RunSink`) so the run itself never
//! touches Tauri. The difference is the **run id**: a search dialog supersedes its own
//! query as the user types, and every event carries the run it belongs to so the
//! frontend can drop the ones that belong to a query it has moved on from
//! (`docs/specs/unindexed-search-plan.md` Decision 11). ❌ The run id is not a
//! cancellation: a superseded run's WALK keeps going, because walking is coverage
//! work and matching is query work.

use serde::{Deserialize, Serialize};
use tauri_specta::Event;

use cmdr_search::run::{RunEnded, RunFailed, RunProgress, RunSink, SearchPhase, SearchRunCoverage, SearchRunError};

use crate::search::types::SearchResultEntry;

/// A batch of results, plus where the run has got to.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "search-progress")]
pub struct SearchProgressEvent {
    /// The run these results belong to. Drop anything naming a run you've
    /// superseded.
    pub run_id: String,
    pub phase: SearchPhase,
    /// The ONE volume every row here lives on, as routing resolved it. Carried
    /// WITH the rows so a caller acting on one (F3, a copy, a drag) never has to
    /// wait for the start reply or the terminal event to know where it is, and
    /// never re-derives it from a path.
    pub target_volume_id: String,
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

// ── Facts onto the wire ──────────────────────────────────────────────

/// The crate's facts carry exactly the event payloads, so each event is its fact
/// in an envelope. Field for field, ❌ never a field computed here: what a run
/// says is decided once, in `cmdr_search::run`.
impl From<RunProgress> for SearchProgressEvent {
    fn from(fact: RunProgress) -> Self {
        let RunProgress {
            run_id,
            phase,
            target_volume_id,
            entries,
            match_count,
            dirs_found,
            current_path,
            capped,
        } = fact;
        Self {
            run_id,
            phase,
            target_volume_id,
            entries,
            match_count,
            dirs_found,
            current_path,
            capped,
        }
    }
}

impl From<RunEnded> for SearchCompleteEvent {
    fn from(
        RunEnded {
            run_id,
            match_count,
            coverage,
        }: RunEnded,
    ) -> Self {
        Self {
            run_id,
            match_count,
            coverage,
        }
    }
}

impl From<RunEnded> for SearchCancelledEvent {
    fn from(
        RunEnded {
            run_id,
            match_count,
            coverage,
        }: RunEnded,
    ) -> Self {
        Self {
            run_id,
            match_count,
            coverage,
        }
    }
}

impl From<RunFailed> for SearchErrorEvent {
    fn from(RunFailed { run_id, error, message }: RunFailed) -> Self {
        Self { run_id, error, message }
    }
}

/// The production sink: `app.emit()` per fact.
pub(crate) struct TauriSearchEventSink {
    app: tauri::AppHandle,
}

impl TauriSearchEventSink {
    pub(crate) fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl RunSink for TauriSearchEventSink {
    fn progress(&self, fact: RunProgress) {
        let _ = SearchProgressEvent::from(fact).emit(&self.app);
    }

    fn complete(&self, fact: RunEnded) {
        let _ = SearchCompleteEvent::from(fact).emit(&self.app);
    }

    fn cancelled(&self, fact: RunEnded) {
        let _ = SearchCancelledEvent::from(fact).emit(&self.app);
    }

    fn failed(&self, fact: RunFailed) {
        let _ = SearchErrorEvent::from(fact).emit(&self.app);
    }
}

/// The test sink: keeps every event, as the wire would carry it, for inspection.
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
impl RunSink for CollectorSearchEventSink {
    fn progress(&self, fact: RunProgress) {
        use crate::ignore_poison::IgnorePoison;
        self.progress.lock_ignore_poison().push(fact.into());
    }

    fn complete(&self, fact: RunEnded) {
        use crate::ignore_poison::IgnorePoison;
        self.complete.lock_ignore_poison().push(fact.into());
    }

    fn cancelled(&self, fact: RunEnded) {
        use crate::ignore_poison::IgnorePoison;
        self.cancelled.lock_ignore_poison().push(fact.into());
    }

    fn failed(&self, fact: RunFailed) {
        use crate::ignore_poison::IgnorePoison;
        self.errors.lock_ignore_poison().push(fact.into());
    }
}

//! A search that keeps going: the runs in flight, the stream their answers go out
//! through, and the one report every run ends on.
//!
//! A live run answers over time (the covered half, then whatever a walk finds), so
//! it needs what a one-shot search doesn't:
//!
//! - **The run registry** ([`SearchRuns`]), a value the host holds. A dialog
//!   supersedes its own query as the user refines it, and every run carries the
//!   token that stops its walk, so a closing dialog or a quitting app can stop every
//!   one of them from wherever it notices.
//! - **[`ResultStream`]**, which turns a trickle of matches into facts at a rate a
//!   UI can absorb: at most [`BATCH_ROWS`] rows per progress fact, at most [`BATCH_INTERVAL`]
//!   between them. It speaks to a [`RunSink`]; the host maps each fact onto its
//!   wire (desktop: four Tauri events, and a fold for MCP).
//! - **The terminal report** ([`CoverageFacts`]): what a run could and couldn't
//!   answer for, built ONE way for every ending, so a fully covered run and a
//!   walked one can't drift in what they claim.
//!
//! What a walk is and how it's pumped stays the host's (desktop: `cmdr-index`'s
//! cover walk, in `apps/desktop/src-tauri/src/search/live.rs`).
//!
//! ## Superseding is not cancelling
//!
//! Refining a query starts a new run and marks the old one superseded: its facts
//! stop going out, and its WALK keeps running to completion filling the index
//! (`docs/specs/unindexed-search-plan.md` Decision 11). Superseding is scoped to the
//! DIALOG, the one asker that retypes; see [`RunOrigin`].

mod registry;
mod report;
mod state;
mod stream;

pub use registry::{LiveRun, RunOrigin, SearchRuns};
pub use report::{CoverageFacts, UnreadableGround, UnresolvedScope};
pub use state::{CoverageKind, SearchPhase, SearchRunCoverage, SearchRunError, WalkEnding};
pub use stream::{BATCH_INTERVAL, BATCH_ROWS, ResultStream, RunEnded, RunFailed, RunProgress, RunSink};

#[cfg(test)]
mod tests;

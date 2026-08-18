//! The coalescer: the stage that turns a flood of changes into something an agent can be told
//! about.
//!
//! Two entry points over ONE fold, because the pipeline has two sources and they must not mean
//! different things:
//!
//! - [`coalesce`] takes individual events (user actions, and any producer that has them).
//! - [`merge_bundles`] takes per-batch, per-folder rollups, which is what the indexer tap hands
//!   over — one message per batch rather than per file, since per-file messages across a crate
//!   boundary would put five million of them on exactly the path the counters exist to survive.
//!
//! Both fold through [`Merger`], so the windowing rule, the deadline anchor, and the ordering
//! are written once. A test asserts the two agree on the same changes.

use std::collections::HashMap;
use std::time::Duration;

use super::{ChangeCounters, EventBundle, FolderEvent};

/// The smallest window that can still divide the timeline. A caller asking for
/// [`Duration::ZERO`] wants no coalescing at all, and per-second bundles are the honest answer
/// to that; dividing by zero would be a crash, and refusing would make the degenerate case the
/// caller's problem for no gain.
const MIN_WINDOW_SECS: u64 = 1;

/// The shared fold: per (window, folder) counters, built in one pass.
///
/// Keyed by a BORROWED folder name, so a folder with a million changes pays for its name once
/// rather than once per change. Nothing here allocates per input beyond the first sighting of
/// each (window, folder) pair.
struct Merger<'a> {
    window_secs: u64,
    bundles: HashMap<(u64, &'a str), EventBundle>,
}

impl<'a> Merger<'a> {
    fn new(window: Duration) -> Self {
        Merger {
            window_secs: window.as_secs().max(MIN_WINDOW_SECS),
            bundles: HashMap::new(),
        }
    }

    /// Fold one contribution in: some counted changes in `folder`, whose activity began at
    /// `anchor` and whose newest change was at `last_event_at`.
    fn add(&mut self, folder: &'a str, anchor: u64, last_event_at: u64, counters: &ChangeCounters) {
        let window_start = (anchor / self.window_secs) * self.window_secs;
        let bundle = self
            .bundles
            .entry((window_start, folder))
            .or_insert_with(|| EventBundle {
                folder: folder.to_string(),
                counters: ChangeCounters::default(),
                window_start,
                last_event_at,
            });
        bundle.counters.merge(counters);
        // The MAX, not the last one seen: contributions arrive out of order, and this is what a
        // deliver-by deadline gets measured from.
        bundle.last_event_at = bundle.last_event_at.max(last_event_at);
    }

    fn finish(self) -> Vec<EventBundle> {
        let mut out: Vec<EventBundle> = self.bundles.into_values().collect();
        // A total order, so two runs over the same input are diffable and a caller can index
        // rather than search. Window first: that's the order the inbox delivers in.
        out.sort_by(|a, b| {
            a.window_start
                .cmp(&b.window_start)
                .then_with(|| a.folder.cmp(&b.folder))
        });
        out
    }
}

/// Fold events into per-folder counters, one bundle per folder per window.
///
/// Windows TUMBLE and are anchored to the epoch (`at / window * window`), never to the first
/// event in the batch. Two consequences the pipeline depends on:
///
/// - The same events coalesce identically however they arrive — out of order, split across
///   batches, or mixed with another folder's noise. An anchor taken from the first event would
///   make the answer depend on what else happened to be in the call.
/// - A morning burst and an evening burst in the same folder can never land in one bundle, so
///   they can never share a deadline. Merged, the later burst would inherit the earlier one's
///   timing and the agent would report tonight's arrivals as this morning's.
///
/// Sub-second windows floor to [`MIN_WINDOW_SECS`], since [`FolderEvent::at`] is whole seconds.
pub fn coalesce(events: &[FolderEvent], window: Duration) -> Vec<EventBundle> {
    let mut merger = Merger::new(window);
    for event in events {
        // A stack value of four counters, so a five-million-event batch allocates nothing per
        // event; the only heap cost is one folder name per (window, folder) pair.
        let mut one = ChangeCounters::default();
        one.record(event.kind);
        merger.add(&event.folder, event.at, event.at, &one);
    }
    merger.finish()
}

/// Fold pre-counted bundles into the same windowed shape [`coalesce`] produces.
///
/// A rollup carries no per-event times, so it is placed by its own `window_start`: two batches
/// straddling a boundary stay two bundles. That is exact as long as one input bundle lies
/// inside one window, which a per-batch rollup does — a live batch spans milliseconds, a
/// coalescing window spans at least a second.
pub fn merge_bundles(bundles: &[EventBundle], window: Duration) -> Vec<EventBundle> {
    let mut merger = Merger::new(window);
    for bundle in bundles {
        merger.add(
            &bundle.folder,
            bundle.window_start,
            bundle.last_event_at,
            &bundle.counters,
        );
    }
    merger.finish()
}

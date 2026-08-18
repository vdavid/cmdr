//! The coalescer: the stage that turns a flood of changes into something an agent can be told
//! about.
//!
//! One pass over the events, folding each into the bundle for its (window, folder). Five
//! million changes in one folder cost one bundle and one counter increment each — never five
//! million of anything retained, which is the whole reason this stage exists ahead of the
//! interest scorer and the digest.

use std::collections::HashMap;
use std::time::Duration;

use super::{ChangeCounters, EventBundle, FolderEvent};

/// The smallest window that can still divide the timeline. A caller asking for
/// [`Duration::ZERO`] wants no coalescing at all, and per-second bundles are the honest answer
/// to that; dividing by zero would be a crash, and refusing would make the degenerate case the
/// caller's problem for no gain.
const MIN_WINDOW_SECS: u64 = 1;

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
    let window_secs = window.as_secs().max(MIN_WINDOW_SECS);
    // Keyed by the window and a BORROWED folder name, so a folder with a million events pays
    // for its name once rather than once per event.
    let mut bundles: HashMap<(u64, &str), EventBundle> = HashMap::new();

    for event in events {
        let window_start = (event.at / window_secs) * window_secs;
        let bundle = bundles
            .entry((window_start, event.folder.as_str()))
            .or_insert_with(|| EventBundle {
                folder: event.folder.clone(),
                counters: ChangeCounters::default(),
                window_start,
                last_event_at: event.at,
            });
        bundle.counters.record(event.kind);
        // The MAX, not the last one seen: events arrive out of order, and this is what a
        // deliver-by deadline gets measured from.
        bundle.last_event_at = bundle.last_event_at.max(event.at);
    }

    let mut out: Vec<EventBundle> = bundles.into_values().collect();
    // A total order, so two runs over the same events are diffable and a caller can index
    // rather than search. Window first: that's the order the inbox delivers in.
    out.sort_by(|a, b| {
        a.window_start
            .cmp(&b.window_start)
            .then_with(|| a.folder.cmp(&b.folder))
    });
    out
}

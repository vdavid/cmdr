//! Wake-pipeline tests.

mod coalesce;
mod compact;
mod interest;

use super::*;

/// One change in `folder` at `at`.
pub(super) fn event(folder: &str, kind: ChangeKind, at: u64) -> FolderEvent {
    FolderEvent {
        folder: folder.to_string(),
        kind,
        at,
    }
}

/// A bundle carrying `counters`, with the window fields the scorer doesn't read pinned to
/// fixed values — the scorer weighs WHAT happened and WHERE, never when.
pub(super) fn bundle(folder: &str, counters: ChangeCounters) -> EventBundle {
    EventBundle {
        folder: folder.to_string(),
        counters,
        window_start: 0,
        last_event_at: 0,
    }
}

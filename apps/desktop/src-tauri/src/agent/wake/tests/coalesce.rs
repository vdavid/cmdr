//! The coalescer: per-folder counters, one bundle per folder per window.

use std::time::Duration;

use super::super::*;
use super::event;

/// The window every test below uses unless it's testing the window itself.
const MINUTE: Duration = Duration::from_secs(60);

/// Find the bundle for a folder, or fail saying which folders DID come back — a wrong-folder
/// bug reads as a missing folder otherwise.
fn bundle_for<'a>(bundles: &'a [EventBundle], folder: &str) -> &'a EventBundle {
    bundles.iter().find(|b| b.folder == folder).unwrap_or_else(|| {
        panic!(
            "no bundle for {folder}; got {:?}",
            bundles.iter().map(|b| &b.folder).collect::<Vec<_>>()
        )
    })
}

/// Changes in one folder inside one window become ONE bundle carrying per-kind counters. This
/// is the whole point of the stage: the agent never sees the individual events.
#[test]
fn changes_in_one_folder_and_window_become_one_counted_bundle() {
    let bundles = coalesce(
        &[
            event("/Users/someone/Downloads", ChangeKind::Created, 100),
            event("/Users/someone/Downloads", ChangeKind::Created, 101),
            event("/Users/someone/Downloads", ChangeKind::Modified, 102),
            event("/Users/someone/Downloads", ChangeKind::Removed, 103),
            event("/Users/someone/Downloads", ChangeKind::Renamed, 104),
        ],
        MINUTE,
    );

    assert_eq!(bundles.len(), 1, "one folder, one window, one bundle: {bundles:?}");
    assert_eq!(
        bundles[0].counters,
        ChangeCounters {
            created: 2,
            modified: 1,
            removed: 1,
            renamed: 1,
        }
    );
    assert_eq!(
        bundles[0].last_event_at, 104,
        "the newest change is what a deadline runs from"
    );
}

/// Two folders never share a bundle, however interleaved their events are: the counters are
/// PER FOLDER, and a merged bundle would file someone's Downloads activity under a build dir.
#[test]
fn different_folders_never_share_a_bundle() {
    let bundles = coalesce(
        &[
            event("/Users/someone/Downloads", ChangeKind::Created, 100),
            event("/tmp/log", ChangeKind::Modified, 100),
            event("/Users/someone/Downloads", ChangeKind::Created, 101),
            event("/tmp/log", ChangeKind::Modified, 101),
        ],
        MINUTE,
    );

    assert_eq!(bundles.len(), 2);
    assert_eq!(bundle_for(&bundles, "/Users/someone/Downloads").counters.created, 2);
    assert_eq!(bundle_for(&bundles, "/tmp/log").counters.modified, 2);
}

/// One folder touched in two different windows produces two bundles, not one merged one. A
/// merge would hand the later burst the earlier one's timing, so "three files arrived this
/// morning" and "three more arrived tonight" would read as one six-file event at breakfast.
#[test]
fn the_same_folder_in_two_windows_produces_two_bundles() {
    let bundles = coalesce(
        &[
            event("/Users/someone/Downloads", ChangeKind::Created, 0),
            event("/Users/someone/Downloads", ChangeKind::Created, 30),
            event("/Users/someone/Downloads", ChangeKind::Created, 3_600),
        ],
        MINUTE,
    );

    assert_eq!(bundles.len(), 2, "{bundles:?}");
    let first = &bundles[0];
    let second = &bundles[1];
    assert_eq!((first.window_start, first.counters.created), (0, 2));
    assert_eq!((second.window_start, second.counters.created), (3_600, 1));
}

/// Windows are anchored to the epoch, not to the first event, so the same events coalesce the
/// same way whatever order they arrive in and whatever else is in the batch.
#[test]
fn windows_are_epoch_anchored_so_input_order_cannot_change_the_answer() {
    let forwards = coalesce(
        &[
            event("/a", ChangeKind::Created, 59),
            event("/a", ChangeKind::Created, 60),
            event("/a", ChangeKind::Created, 119),
        ],
        MINUTE,
    );
    let backwards = coalesce(
        &[
            event("/a", ChangeKind::Created, 119),
            event("/a", ChangeKind::Created, 60),
            event("/a", ChangeKind::Created, 59),
        ],
        MINUTE,
    );

    assert_eq!(forwards, backwards, "input order must not change the bundles");
    assert_eq!(forwards.len(), 2, "59 and 60 straddle a window boundary: {forwards:?}");
    assert_eq!(forwards[0].window_start, 0);
    assert_eq!(forwards[1].window_start, 60);
}

/// Bundles come back in a deterministic order — by window, then by folder — so a caller can
/// diff two runs, and so the tests above can index rather than search.
#[test]
fn bundles_come_back_ordered_by_window_then_folder() {
    let bundles = coalesce(
        &[
            event("/z", ChangeKind::Created, 3_600),
            event("/b", ChangeKind::Created, 0),
            event("/a", ChangeKind::Created, 0),
            event("/a", ChangeKind::Created, 3_600),
        ],
        MINUTE,
    );

    assert_eq!(
        bundles
            .iter()
            .map(|b| (b.window_start, b.folder.as_str()))
            .collect::<Vec<_>>(),
        [(0, "/a"), (0, "/b"), (3_600, "/a"), (3_600, "/z")]
    );
}

/// No events in, no bundles out. The quiet case is the common one: if nothing interesting
/// happened, the agent simply does not run.
#[test]
fn no_events_produce_no_bundles() {
    assert!(coalesce(&[], MINUTE).is_empty());
}

/// A zero window can't divide the timeline, so it degrades to the smallest real one (a second)
/// rather than panicking. A caller passing `Duration::ZERO` is asking for no coalescing, and
/// the honest answer is per-second bundles, not a crash.
#[test]
fn a_zero_window_degrades_to_one_second_instead_of_dividing_by_zero() {
    let bundles = coalesce(
        &[
            event("/a", ChangeKind::Created, 10),
            event("/a", ChangeKind::Created, 10),
            event("/a", ChangeKind::Created, 11),
        ],
        Duration::ZERO,
    );

    assert_eq!(bundles.len(), 2, "{bundles:?}");
    assert_eq!(bundles[0].counters.created, 2);
    assert_eq!(bundles[1].counters.created, 1);
}

/// Five million changes in one folder coalesce to one bundle whose counter is exact. This is
/// the noise case the deterministic layer exists to absorb: it must cost one bundle, not five
/// million of anything.
#[test]
fn five_million_changes_in_one_folder_coalesce_to_one_exact_bundle() {
    let events: Vec<FolderEvent> = (0..5_000_000u64)
        .map(|i| event("/tmp/log", ChangeKind::Modified, 10 + i % 60))
        .collect();

    let bundles = coalesce(&events, Duration::from_secs(300));

    assert_eq!(bundles.len(), 1, "one folder, one window: {}", bundles.len());
    assert_eq!(bundles[0].counters.modified, 5_000_000);
    assert_eq!(bundles[0].counters.total(), 5_000_000);
}

// ── Merging pre-counted bundles ───────────────────────────────────────────────

/// The tap hands over per-batch, per-folder ROLLUPS rather than one message per file, so the
/// coalescer has to accept counted input as well as individual events. Both must land on the
/// same answer, or the pipeline means something different depending on which source fed it.
#[test]
fn merging_pre_counted_bundles_matches_coalescing_the_same_events() {
    let events = [
        event("/Users/someone/Downloads", ChangeKind::Created, 10),
        event("/Users/someone/Downloads", ChangeKind::Created, 20),
        event("/Users/someone/Downloads", ChangeKind::Modified, 70),
        event("/tmp/log", ChangeKind::Modified, 15),
    ];
    // The same changes, pre-counted the way a live batch would hand them over.
    let per_event: Vec<EventBundle> = events
        .iter()
        .map(|e| {
            let mut counters = ChangeCounters::default();
            counters.record(e.kind);
            EventBundle {
                folder: e.folder.clone(),
                counters,
                window_start: e.at,
                last_event_at: e.at,
            }
        })
        .collect();

    assert_eq!(coalesce(&events, MINUTE), merge_bundles(&per_event, MINUTE));
}

/// Two rollups for one folder in one window become one bundle: counters summed, and the
/// deadline anchored to the LATER of the two.
#[test]
fn two_rollups_for_one_folder_in_one_window_become_one() {
    let first = EventBundle {
        folder: "/Users/someone/Downloads".to_string(),
        counters: ChangeCounters {
            created: 3,
            ..ChangeCounters::default()
        },
        window_start: 10,
        last_event_at: 12,
    };
    let second = EventBundle {
        folder: "/Users/someone/Downloads".to_string(),
        counters: ChangeCounters {
            created: 2,
            modified: 1,
            ..ChangeCounters::default()
        },
        window_start: 40,
        last_event_at: 44,
    };

    let merged = merge_bundles(&[first, second], MINUTE);

    assert_eq!(merged.len(), 1, "{merged:?}");
    assert_eq!(merged[0].counters.created, 5);
    assert_eq!(merged[0].counters.modified, 1);
    assert_eq!(merged[0].window_start, 0, "both sit in the epoch-anchored first minute");
    assert_eq!(merged[0].last_event_at, 44, "the deadline runs from the newest change");
}

/// A rollup carries no per-event times, so it can only be placed by the window its own start
/// falls in. Two batches straddling a boundary therefore stay two bundles — the same answer
/// the per-event path gives, and the reason the tap emits per batch rather than per minute.
#[test]
fn rollups_either_side_of_a_boundary_stay_apart() {
    let before = EventBundle {
        folder: "/a".to_string(),
        counters: ChangeCounters {
            created: 1,
            ..ChangeCounters::default()
        },
        window_start: 59,
        last_event_at: 59,
    };
    let after = EventBundle {
        folder: "/a".to_string(),
        counters: ChangeCounters {
            created: 1,
            ..ChangeCounters::default()
        },
        window_start: 60,
        last_event_at: 60,
    };

    let merged = merge_bundles(&[before, after], MINUTE);

    assert_eq!(merged.len(), 2, "{merged:?}");
    assert_eq!((merged[0].window_start, merged[1].window_start), (0, 60));
}

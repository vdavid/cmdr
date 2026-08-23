//! The app half of the tap, end to end: an `IndexEvent::FolderActivity` through `route()` into
//! the channel, the window, and the inbox.
//!
//! The crate half (a synthetic live batch through `process_live_batch`) can't live here:
//! `process_live_batch` is `pub(in crate::indexing)` and `cmdr-index` may never name the
//! agent. It is `crates/cmdr-index/src/indexing/watch/event_loop/tests/activity.rs`.

use cmdr_index::{FolderChangeRollup, IndexEvent};

use super::super::channel::{self, WakeMessage};
use super::super::{DEFAULT_HOT_DELAY, FolderImportance, Inbox, MAX_WARM_DELAY, WAKE_WINDOW, WakeReadiness};
use crate::events::index_mapping::{Destination, route};

/// One rollup as the index would report it.
fn rollup(folder: &str, created: u32, renamed: u32, last_event_at: u64) -> FolderChangeRollup {
    FolderChangeRollup {
        folder: folder.to_string(),
        created,
        modified: 0,
        removed: 0,
        renamed,
        last_event_at,
    }
}

/// A path prefix nothing else in the test binary uses. `index_mapping`'s completeness test
/// routes a sample `FolderActivity` through the same process-global channel, so this test
/// filters to its OWN rollups rather than assuming it is alone in there.
const TAP_ROOT: &str = "/Users/cmdrtap/";

/// This test's rollups, waiting in the channel, taking the receiver.
///
/// ⚠️ The receiver is a process-global handed out ONCE, so exactly one test in the binary may
/// claim it. Keep that here: a second claimer would get `None` and read as a mystery failure.
fn drain_rollups() -> Vec<channel::FolderActivity> {
    let receiver = channel::take_receiver().expect("the wake loop does not run in unit tests");
    receiver
        .try_iter()
        .filter_map(|message| match message {
            WakeMessage::Rollup(activity) => Some(activity),
            WakeMessage::Control(_) => None,
        })
        .filter(|activity| activity.folder.starts_with(TAP_ROOT))
        .collect()
}

/// ⚠️ **`route(event, None)`, which is the whole point of this half.** `route` takes
/// `Option<&AppHandle>`, and the tap handler must reach the wake loop through the process-global
/// channel rather than `app.state()`: managed state doesn't exist yet during launch replay, the
/// busiest window the tap will ever see, and would silently drop every rollup here too.
///
/// The rest is the mapping and the window: counters and `last_event_at` cross untouched, the
/// batch instant floors to the wake window, and the row the inbox opens is scored and given a
/// deadline.
#[test]
fn a_folder_activity_event_reaches_the_inbox_through_the_channel() {
    // 1_780_000_020 is a window boundary, so a batch instant of +27 floors back onto it.
    let event = IndexEvent::FolderActivity {
        volume_id: "root".to_string(),
        observed_at: 1_780_000_027,
        folders: vec![
            rollup("/Users/cmdrtap/Downloads", 3, 1, 1_780_000_027),
            rollup("/Users/cmdrtap/Documents", 1, 0, 1_780_000_026),
        ],
    };

    assert_eq!(
        route(event, None),
        Destination::AgentWake,
        "❌ never AnalyticsOnly: the destination enum's job is saying where an event went"
    );

    let mut activities = drain_rollups();
    activities.sort_by(|a, b| a.folder.cmp(&b.folder));
    assert_eq!(activities.len(), 2, "one message per folder, never per file");
    assert_eq!(activities[0].folder, "/Users/cmdrtap/Documents");
    assert_eq!(activities[1].volume_id, "root", "the volume rides along for the lookup");
    assert_eq!(activities[1].counters.created, 3);
    assert_eq!(activities[1].counters.renamed, 1);
    assert_eq!(activities[1].observed_at, 1_780_000_027, "the batch's own instant");
    assert_eq!(activities[1].last_event_at, 1_780_000_027);

    // The APP quantizes, never the crate. Without this every ~1 s batch becomes its own row.
    let bundle = activities[1].clone().into_bundle();
    assert_eq!(bundle.window_start, 1_780_000_020);
    assert_eq!(bundle.window_start % WAKE_WINDOW.as_secs(), 0);
    assert_eq!(
        bundle.last_event_at, 1_780_000_027,
        "the newest change is NOT quantized"
    );

    let mut inbox = Inbox::default();
    let admitted = inbox.admit_if_permitted(
        WakeReadiness::Ready,
        bundle,
        FolderImportance::Unknown,
        DEFAULT_HOT_DELAY,
        1_780_000_030,
    );

    assert!(admitted, "a ready agent stores what the tap saw");
    assert_eq!(inbox.rows().len(), 1);
    assert_eq!(inbox.rows()[0].bundle.counters.created, 3);
    assert!(
        inbox.rows()[0].interest.value() > 0.0,
        "❌ never Interest(0.0): a batch of creates and a rename is exactly what the agent exists to notice"
    );
    // A folder the scorer hasn't reached lands WARM at the default cadence, which derives as
    // `hot × 60`. ❌ Not `None`: cold is what has no deadline at all, and a row that never
    // comes due is the failure this whole path exists to avoid.
    let warm = DEFAULT_HOT_DELAY * 60;
    assert!(warm < MAX_WARM_DELAY, "well inside the warm cap at the default cadence");
    assert_eq!(
        inbox.next_deadline(),
        Some(1_780_000_030 + warm.as_secs()),
        "and it comes due on a cadence derived from the user's setting"
    );
}

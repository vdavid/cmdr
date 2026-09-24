//! A change batch's follow-up stats run together, and the listing still hears
//! the changes in the order the server sent them.
//!
//! A busy NAS holds a single `stat` for up to 6 s while its session stays
//! healthy. Asked one after another, that one stall delayed every later change
//! in the batch by the same 6 s; asked together, the batch takes as long as its
//! slowest stat. The `stat` here is injected (`process_event_batch_with`) and
//! answers after a per-name delay on a paused clock, so no server is involved.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::DirectoryChange;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::listings::RecordingListings;
use smb2::FileNotifyAction;

use super::super::MountAnchor;
use super::process_event_batch_with;

const MOUNT: &str = "/Volumes/fixture-share";
const VOLUME_ID: &str = "smb-stat-concurrency-test";

/// Runs one directory's events through the batch processor against a server
/// that answers each `stat` after `delay_for(name)`, and hands back what the
/// listing heard, in order, plus how long the batch took.
async fn run(events: Vec<(FileNotifyAction, &str)>, delay_for: fn(&str) -> Duration) -> (Vec<String>, Duration) {
    let listings = Arc::new(RecordingListings::new());
    let host = VolumeHost::builder().listings(listings.clone()).build();
    let batch = HashMap::from([(
        PathBuf::from(MOUNT),
        events
            .into_iter()
            .map(|(action, name)| (action, name.to_string()))
            .collect(),
    )]);
    let stat = |path: PathBuf| async move {
        let name = path.file_name()?.to_string_lossy().to_string();
        // allowed-test-sleep: the fake latency IS the server under test
        tokio::time::sleep(delay_for(&name)).await;
        Some(FileEntry::new(name, path.to_string_lossy().to_string(), false, false))
    };

    let started = tokio::time::Instant::now();
    process_event_batch_with(&host, batch, VOLUME_ID, &MountAnchor::at_share_root(MOUNT), stat).await;
    let took = started.elapsed();

    let heard = listings
        .changes()
        .into_iter()
        .map(|(_, dir, change)| {
            assert_eq!(dir, Path::new(MOUNT));
            match change {
                DirectoryChange::Added(e) => format!("added {}", e.name),
                DirectoryChange::Removed(name) => format!("removed {name}"),
                DirectoryChange::Modified(e) => format!("modified {}", e.name),
                DirectoryChange::Renamed { old_name, new_entry } => format!("renamed {old_name} -> {}", new_entry.name),
                _ => "another kind of change".to_string(),
            }
        })
        .collect();
    (heard, took)
}

fn three_seconds(_: &str) -> Duration {
    Duration::from_secs(3)
}

/// Three changes whose stats each take 3 s arrive after 3 s, not 9.
#[tokio::test(start_paused = true)]
async fn a_batch_takes_as_long_as_its_slowest_stat() {
    let (heard, took) = run(
        vec![
            (FileNotifyAction::Added, "a.txt"),
            (FileNotifyAction::Modified, "b.txt"),
            (FileNotifyAction::Added, "c.txt"),
        ],
        three_seconds,
    )
    .await;

    assert_eq!(heard, ["added a.txt", "modified b.txt", "added c.txt"]);
    assert!(
        took < Duration::from_secs(4),
        "the stats ran one after another: {took:?}"
    );
}

/// A slow first stat doesn't let a fast later one overtake it, and the changes
/// that need no stat (a removal, a rename's old half) keep their place.
#[tokio::test(start_paused = true)]
async fn the_listing_hears_the_changes_in_the_servers_order() {
    fn first_is_slow(name: &str) -> Duration {
        if name == "slow.txt" {
            Duration::from_secs(6)
        } else {
            Duration::ZERO
        }
    }
    let (heard, _) = run(
        vec![
            (FileNotifyAction::Added, "slow.txt"),
            (FileNotifyAction::Removed, "gone.txt"),
            (FileNotifyAction::RenamedOldName, "before.txt"),
            (FileNotifyAction::RenamedNewName, "after.txt"),
            (FileNotifyAction::Added, "fast.txt"),
        ],
        first_is_slow,
    )
    .await;

    assert_eq!(
        heard,
        [
            "added slow.txt",
            "removed gone.txt",
            "renamed before.txt -> after.txt",
            "added fast.txt"
        ]
    );
}

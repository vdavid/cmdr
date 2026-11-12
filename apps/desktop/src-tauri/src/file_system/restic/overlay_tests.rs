//! What the restic contributor claims: a repo's listing on any unrouted volume,
//! and nothing else.
//!
//! The seam's own rules live in `crates/cmdr-listing/src/host/overlays_test.rs`;
//! the walker-exposure consequences in `walker_exposure_tests.rs`.

use std::path::Path;
use std::sync::Arc;

use super::overlay::ResticOverlay;
use super::wiring;
use crate::file_system::volume::{InMemoryVolume, Volume};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::restic_view::VIRTUAL_ENTRY_NAME;
use cmdr_listing::host::ListingOverlay;

const REPO: &str = "/share/backups/test-restic";

fn entry(name: &str, is_directory: bool) -> FileEntry {
    FileEntry::new(name.to_string(), format!("{REPO}/{name}"), is_directory, false)
}

fn repo_entries() -> Vec<FileEntry> {
    let mut entries = vec![entry("config", false)];
    entries.extend(["data", "index", "keys", "snapshots", "locks"].map(|name| entry(name, true)));
    entries
}

/// A repo on a share gets the row: the repo is read through `Volume`, so any
/// unrouted backend qualifies, unlike the git portal's local-only rule.
#[test]
fn a_repo_listing_on_any_unrouted_volume_gets_the_row() {
    let share = InMemoryVolume::new("Share");
    assert!(!share.routes_over_a_parent());
    assert!(ResticOverlay.applies_to(&share, Path::new(REPO), &repo_entries()));
}

/// A folder that isn't a repo gets nothing, and neither does one too big to be
/// a repo root, whatever its names.
#[test]
fn a_plain_folder_and_a_big_folder_get_no_row() {
    let volume = InMemoryVolume::new("Disk");
    let plain = vec![entry("config", false), entry("data", true)];
    assert!(!ResticOverlay.applies_to(&volume, Path::new(REPO), &plain));

    let mut big = repo_entries();
    big.extend((0..wiring::MAX_REPO_ROOT_ENTRIES).map(|index| entry(&format!("extra{index}"), false)));
    assert!(!ResticOverlay.applies_to(&volume, Path::new(REPO), &big));
}

/// A repo inside a snapshot or an archive is backed-up bytes: no second view
/// nests under a routed volume.
#[tokio::test]
async fn a_repo_seen_through_a_routed_volume_gets_no_row() {
    let parent: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Share"));
    let view = cmdr_restic::ResticVolume::new(
        Arc::clone(wiring::repos()),
        "overlay-routed",
        parent,
        Path::new(REPO).to_path_buf(),
    );
    assert!(view.routes_over_a_parent());
    assert!(!ResticOverlay.applies_to(&view, Path::new(REPO), &repo_entries()));
}

/// The row is a folder named for the view, at the path the route serves, and
/// adding it confirms the repo for the volume id the listing was read under.
#[test]
fn the_row_points_at_the_view_and_confirms_the_repo() {
    let volume = InMemoryVolume::new("Share");
    let rows = ResticOverlay.extra_entries("overlay-seeds", &volume, Path::new(REPO));

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, VIRTUAL_ENTRY_NAME);
    assert!(rows[0].is_directory);
    assert_eq!(rows[0].path, format!("{REPO}/{VIRTUAL_ENTRY_NAME}"));
    assert_eq!(
        wiring::repos().serves_confirmed("overlay-seeds", Path::new(REPO)),
        Some(true)
    );
    assert_eq!(
        wiring::repos().serves_confirmed("another-volume", Path::new(REPO)),
        None,
        "keyed by the volume the listing came from"
    );
}

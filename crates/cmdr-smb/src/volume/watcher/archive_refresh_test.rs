//! SMB watcher → archive-inner refresh routing.
//!
//! The recursive share watch already refreshes the DIRECTORY listing showing a
//! changed `.zip`. These tests pin the added behavior: when the changed path is
//! a supported archive, `process_event_batch` ALSO asks the listing seam to
//! refresh any open listing INSIDE the archive — the push-refresh a REMOTE
//! parent otherwise never gets, since `archive::watch` (the local-parent
//! equivalent) can't arm without a local `notify` transport. A non-archive
//! change must NOT reach the seam (the extension gate).
//!
//! What a refresh DOES to the listing cache is the app's half, pinned by
//! `listing/listing_host.rs::the_archive_refresh_re_reads_the_listings_under_its_path`.
//! Here the seam is a [`RecordingListings`], so these are routing assertions
//! with no archive, no filesystem, and no app in them.
//!
//! Every case runs with a dead share handle, which is what they want: there is
//! no SMB session, so each `stat_via_share` answers `None` and the batch takes
//! its "couldn't stat, skipping" arm. The archive-inner refresh under test is
//! deliberately independent of that stat.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmdr_fs::volume::SelfHandle;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::listings::RecordingListings;
use smb2::FileNotifyAction;

use super::process_event_batch;

/// The mount root every case builds its paths under. Never touched on disk.
const MOUNT: &str = "/Volumes/fixture-share";

/// The volume id the notifications are keyed on.
const VOLUME_ID: &str = "smb-archive-refresh-test";

/// A host whose only live seam is the listing recorder these tests read back.
fn recording_host() -> (VolumeHost, Arc<RecordingListings>) {
    let listings = Arc::new(RecordingListings::new());
    let host = VolumeHost::builder().listings(listings.clone()).build();
    (host, listings)
}

/// A share handle that has already gone. See the module docs for why that's the
/// state under test.
fn no_live_share() -> SelfHandle<super::SmbVolumeInner> {
    SelfHandle::new(std::sync::Weak::new())
}

/// One event naming `filename` under the mount root, ready for
/// `process_event_batch`.
fn batch(action: FileNotifyAction, filename: &str) -> HashMap<PathBuf, Vec<(FileNotifyAction, String)>> {
    HashMap::from([(PathBuf::from(MOUNT), vec![(action, filename.to_string())])])
}

/// Drives one event through the batch processor and answers every archive
/// refresh it asked for.
async fn archive_refreshes_for(action: FileNotifyAction, filename: &str) -> Vec<(String, PathBuf)> {
    let (host, listings) = recording_host();
    process_event_batch(
        &host,
        batch(action, filename),
        VOLUME_ID,
        &no_live_share(),
        Path::new(MOUNT),
    )
    .await;
    listings.archive_refreshes()
}

/// A `Modified` event for a backing `.zip` (an in-place rewrite) asks for the
/// archive-inner refresh, keyed on the parent drive's volume id.
#[tokio::test]
async fn a_modified_archive_event_asks_for_the_inner_refresh() {
    assert_eq!(
        archive_refreshes_for(FileNotifyAction::Modified, "bundle.zip").await,
        vec![(VOLUME_ID.to_string(), PathBuf::from(MOUNT).join("bundle.zip"))],
        "a Modified event for a supported archive must reach the archive-refresh seam"
    );
}

/// A temp+rename swap over the backing `.zip` (the editor / safe-overwrite path)
/// asks for it too: the bytes changed the same way, only the syscall differs.
#[tokio::test]
async fn a_renamed_archive_event_asks_for_the_inner_refresh() {
    assert_eq!(
        archive_refreshes_for(FileNotifyAction::RenamedNewName, "bundle.zip").await,
        vec![(VOLUME_ID.to_string(), PathBuf::from(MOUNT).join("bundle.zip"))],
        "a rename onto a supported archive must reach the archive-refresh seam"
    );
}

/// A `Modified` event for a NON-archive sibling reaches nothing (the extension
/// gate holds).
#[tokio::test]
async fn a_modified_non_archive_event_asks_for_nothing() {
    assert!(
        archive_refreshes_for(FileNotifyAction::Modified, "notes.txt")
            .await
            .is_empty(),
        "a non-archive change must not reach the archive-refresh seam"
    );
}

/// An `Added` event doesn't: a `.zip` that has only just appeared can have no
/// listing open inside it yet.
#[tokio::test]
async fn an_added_archive_event_asks_for_nothing() {
    assert!(
        archive_refreshes_for(FileNotifyAction::Added, "bundle.zip")
            .await
            .is_empty(),
        "an archive that has only just appeared has no inner listing to refresh"
    );
}

/// The path the seam gets is the NFD display path, matching every other
/// cache-facing path the watcher builds. The server sends NFC; a listing cached
/// under the macOS mount's NFD key would never be found under it.
#[tokio::test]
async fn the_refreshed_path_is_normalized_the_way_the_cache_keys_are() {
    // "café.zip" with a precomposed é (U+00E9), the way a server reports it.
    let refreshes = archive_refreshes_for(FileNotifyAction::Modified, "caf\u{00e9}.zip").await;
    assert_eq!(
        refreshes,
        // …and decomposed (e + U+0301) on the way to the cache.
        vec![(VOLUME_ID.to_string(), PathBuf::from(MOUNT).join("cafe\u{0301}.zip"))],
        "the refreshed path must carry the same NFD normalization the listing cache keys on"
    );
}

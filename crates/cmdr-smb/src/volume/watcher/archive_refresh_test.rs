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
use std::path::PathBuf;
use std::sync::Arc;

use cmdr_fs::volume::SelfHandle;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::listings::RecordingListings;
use smb2::FileNotifyAction;

use super::super::MountAnchor;
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
    refreshes_through(MountAnchor::at_share_root(MOUNT), action, filename).await
}

/// Same, for a mount anchored inside the share: the filename the server sends is
/// SHARE-relative, so it carries the anchor.
async fn refreshes_through(anchor: MountAnchor, action: FileNotifyAction, filename: &str) -> Vec<(String, PathBuf)> {
    let (host, listings) = recording_host();
    process_event_batch(&host, batch(action, filename), VOLUME_ID, &no_live_share(), &anchor).await;
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

/// The path the seam gets carries the server's own bytes, in whichever Unicode
/// form it sent them, because that's what the pane's path for the archive holds
/// (it came out of a listing) and the cache compares the two byte-for-byte.
#[tokio::test]
async fn the_refreshed_path_keeps_the_servers_own_bytes() {
    // "café.zip" composed (U+00E9) and decomposed (e + U+0301): a server can
    // store either, and each must reach the cache exactly as sent.
    for name in ["caf\u{00e9}.zip", "cafe\u{0301}.zip"] {
        assert_eq!(
            archive_refreshes_for(FileNotifyAction::Modified, name).await,
            vec![(VOLUME_ID.to_string(), PathBuf::from(MOUNT).join(name))],
            "{name:?} must reach the listing cache byte-for-byte"
        );
    }
}

// ── Mounts anchored inside the share ──────────────────────────────────────────
//
// The watch is ONE recursive `CHANGE_NOTIFY` on the share root, so its event
// paths are share-relative and a mount anchored inside the share hears about the
// whole share. Both halves matter: paths inside the anchor must lose it, and
// paths outside it belong to no listing here.

/// An event inside the anchor is reported at its path under THIS mount, with the
/// anchor taken off. Leaving it on names `/Volumes/SYSVOL/domain/domain/x`, which
/// no pane has open, so the listing that did change silently stays stale.
#[tokio::test]
async fn an_event_inside_the_anchor_is_reported_under_this_mount() {
    let refreshes = refreshes_through(
        MountAnchor::new(MOUNT, "team/archive"),
        FileNotifyAction::Modified,
        "team/archive/bundle.zip",
    )
    .await;
    assert_eq!(
        refreshes,
        vec![(VOLUME_ID.to_string(), PathBuf::from(MOUNT).join("bundle.zip"))]
    );
}

/// An event elsewhere in the share is not this mount's business. Joining it on
/// anyway would invalidate a path that doesn't exist under this mount.
#[tokio::test]
async fn an_event_outside_the_anchor_is_ignored() {
    for elsewhere in ["other/bundle.zip", "team/archive-old/bundle.zip", "bundle.zip"] {
        assert!(
            refreshes_through(
                MountAnchor::new(MOUNT, "team/archive"),
                FileNotifyAction::Modified,
                elsewhere
            )
            .await
            .is_empty(),
            "{elsewhere} is outside this mount's anchor"
        );
    }
}

/// `MountAnchor::new` folds the anchor to NFC, the spelling the macOS kernel
/// mount itself used to reach it, so a decomposed `statfs` anchor still strips
/// off an event path the server sends composed.
#[tokio::test]
async fn an_accented_anchor_still_matches_what_the_server_sends() {
    let refreshes = refreshes_through(
        // Anchor given decomposed, the way macOS `statfs` spells it…
        MountAnchor::new(MOUNT, "arme\u{0301}e"),
        FileNotifyAction::Modified,
        // …and composed, the way the server reports the event.
        "arm\u{00e9}e/bundle.zip",
    )
    .await;
    assert_eq!(
        refreshes,
        vec![(VOLUME_ID.to_string(), PathBuf::from(MOUNT).join("bundle.zip"))],
        "MountAnchor::new folds the anchor to NFC, which is what the event path is in"
    );
}

/// A stream event is a change to the file that holds it, under the file's own name, so it
/// takes the `Modified` path and reaches the listing host's re-read.
#[test]
fn a_stream_event_folds_into_a_change_on_its_file() {
    use super::fold_stream_event;
    for action in [
        FileNotifyAction::StreamAdded,
        FileNotifyAction::StreamRemoved,
        FileNotifyAction::StreamModified,
    ] {
        assert_eq!(
            fold_stream_event(action, "a/f.txt:com.apple.metadata_kMDItemUserTags"),
            (FileNotifyAction::Modified, "a/f.txt")
        );
        assert_eq!(
            fold_stream_event(action, "a/f.txt"),
            (FileNotifyAction::Modified, "a/f.txt")
        );
    }
    assert_eq!(
        fold_stream_event(FileNotifyAction::Added, "a/f.txt"),
        (FileNotifyAction::Added, "a/f.txt")
    );
}

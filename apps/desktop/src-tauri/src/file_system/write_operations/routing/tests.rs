//! The two resolution questions every transfer asks before it starts.
//!
//! Both answers are load-bearing and neither is obvious from its inputs: an
//! unanchored destination reaches a share as a path outside its mount, and a
//! `.zip` FILE routed to its own `ArchiveVolume` would scan the archive's contents
//! instead of copying the file.

use super::{resolve_dest_path, resolve_source_volume};
use crate::file_system::volume::manager::RoutedKind;
use cmdr_fs::volume::{InMemoryVolume, Volume};
use std::path::Path;
use std::sync::Arc;

/// A non-local volume (no `local_path`) rooted where a mounted share is: stands in
/// for `SmbVolume` without a network.
fn share_at(root: &str) -> Arc<dyn Volume> {
    Arc::new(InMemoryVolume::new("TestShare").with_root(root))
}

#[test]
fn a_remote_destination_is_anchored_at_its_mount_root() {
    // The dialog's box is volume-relative. Unanchored, `/_todo_pics` reaches
    // `SmbVolume` as an absolute path outside the mount and comes back
    // `NotFound` before any I/O, which is what killed a 360 GB move
    // (ERR-XCP5Q) and reported the DESTINATION as a missing source.
    let volume = share_at("/Volumes/naspi");
    assert_eq!(
        resolve_dest_path(&volume, "/_todo_pics/Fiumei footage".to_string()),
        Path::new("/Volumes/naspi/_todo_pics/Fiumei footage")
    );
}

#[test]
fn a_remote_destination_already_under_its_root_is_untouched() {
    let volume = share_at("/Volumes/naspi");
    assert_eq!(
        resolve_dest_path(&volume, "/Volumes/naspi/_todo_pics".to_string()),
        Path::new("/Volumes/naspi/_todo_pics")
    );
}

#[test]
fn a_remote_destination_at_the_share_root_stays_the_root() {
    // The one shape that worked before anchoring existed; it must keep
    // working, and must not become `/Volumes/naspi/`.
    let volume = share_at("/Volumes/naspi");
    assert_eq!(resolve_dest_path(&volume, "/".to_string()), Path::new("/Volumes/naspi"));
}

#[test]
fn a_local_destination_still_expands_the_home_shortcut() {
    let volume: Arc<dyn Volume> = Arc::new(crate::file_system::volume::LocalPosixVolume::new("Root", "/"));
    let home = std::env::var("HOME").expect("HOME");
    assert_eq!(
        resolve_dest_path(&volume, "~/Downloads".to_string()),
        Path::new(&home).join("Downloads")
    );
}

#[tokio::test]
async fn resolve_source_treats_the_zip_file_itself_as_a_plain_file() {
    use crate::file_system::volume::LocalPosixVolume;
    use crate::file_system::volume::manager::get_volume_manager;

    let dir = tempfile::tempdir().expect("tempdir");
    let zip = dir.path().join("bundle.zip");
    std::fs::write(&zip, b"PK\x03\x04rest").expect("write zip magic");
    // The parent drive holds the `.zip`. (nextest isolates the global per test.)
    get_volume_manager().register(
        "root",
        Arc::new(LocalPosixVolume::new("Root", dir.path().to_str().unwrap())),
    );

    // The `.zip` FILE itself is copied as a plain file: routed to the PARENT
    // volume, `is_inside = false` (NOT the ArchiveVolume, which would scan its
    // contents instead of copying the file).
    let (vol, route) = resolve_source_volume("root", Some(&zip)).await.expect("source volume");
    assert_eq!(route, None, "the .zip file itself is not archive-inner");
    assert_eq!(vol.name(), "Root", "routed to the parent volume, not the archive");

    // A path INSIDE the archive routes to the ArchiveVolume. This is what makes
    // an EXTRACT reachable as an ordinary copy.
    let (inner_vol, inner_route) = resolve_source_volume("root", Some(&zip.join("entry.txt")))
        .await
        .expect("inner volume");
    assert_eq!(inner_route, Some(RoutedKind::Archive), "an inner path is archive-inner");
    assert_eq!(inner_vol.root(), zip, "the archive volume's root is the .zip");
}

/// A snapshot path is the portal's, so the copy engine reads it with
/// `open_read_stream` and walks it with `scan_for_copy`. Left on the parent
/// volume it would take the local-to-local fast path against a path with no
/// inode: the transfer dialog then sits on "Verifying before copy" forever and
/// ends in "Couldn't finish copying".
#[tokio::test]
async fn resolve_source_routes_a_snapshot_path_to_the_git_portal() {
    use crate::file_system::git;
    use crate::file_system::volume::LocalPosixVolume;
    use crate::file_system::volume::manager::get_volume_manager;
    use cmdr_git::test_fixtures::{Fixture, cleanup, temp_dir};

    let dir = temp_dir("write_routing", "copy_out");
    let mut fixture = Fixture::init(dir.clone());
    fixture.commit_file("README.md", b"hello\n", "initial");
    get_volume_manager().register("root", Arc::new(LocalPosixVolume::new("Root", dir.to_str().unwrap())));
    git::wiring::set_virtual_portal_enabled(true);

    let (volume, route) = resolve_source_volume("root", Some(&dir.join(".git/branches/main/README.md")))
        .await
        .expect("source volume");
    assert_eq!(route, Some(RoutedKind::GitPortal));
    assert_eq!(volume.name(), ".git", "the portal serves it, not the parent drive");
    assert_eq!(
        volume.local_path(),
        None,
        "no local path, so the copy takes the cross-volume engine"
    );

    // `.git/` itself and the real files under it are the parent's, so editing
    // and deleting them keeps working.
    let (parent, real_route) = resolve_source_volume("root", Some(&dir.join(".git/config")))
        .await
        .expect("source volume");
    assert_eq!(real_route, None);
    assert_eq!(parent.name(), "Root");

    cleanup(&dir);
}

/// A `NotConnected` a backend answers mid-transfer names the half that asked, the
/// same way an unregistered volume does before the transfer starts. ❌ Never
/// `DeviceDisconnected`: nothing dropped.
#[test]
fn a_volume_that_isnt_connected_yet_maps_to_the_side_that_asked() {
    use crate::file_system::write_operations::WriteOperationError;
    use crate::file_system::write_operations::transfer::volume::{PathRole, map_volume_error};
    use cmdr_fs::volume::VolumeError;

    let not_connected = || VolumeError::NotConnected("adb://R58M/sdcard".to_string());
    assert!(matches!(
        map_volume_error("adb://R58M/sdcard", PathRole::Destination, not_connected()),
        WriteOperationError::DestinationNotConnected { .. }
    ));
    assert!(matches!(
        map_volume_error("adb://R58M/sdcard", PathRole::Source, not_connected()),
        WriteOperationError::SourceNotConnected { .. }
    ));
}

/// An id no provider lists and no saved server names keeps reading as a volume
/// that's gone (an unmount race), ❌ never "not connected yet": there's nothing to
/// open, so that advice would send the user looking for a row that isn't there.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_onto_an_id_nothing_knows_still_reads_as_a_missing_volume() {
    use crate::file_system::volume::manager::get_volume_manager;
    use crate::file_system::write_operations::event_sinks::CollectorEventSink;
    use crate::file_system::write_operations::{VolumeCopyConfig, WriteOperationError, start_volume_copy};
    use crate::operation_log::types::Initiator;

    let source_id = format!("routing-source-{}", uuid::Uuid::new_v4());
    let source = InMemoryVolume::new("Source");
    source
        .create_file(Path::new("/a.txt"), b"a")
        .await
        .expect("the source file");
    get_volume_manager().register(&source_id, Arc::new(source));

    let refused = start_volume_copy(
        Arc::new(CollectorEventSink::new()),
        source_id.clone(),
        vec![Path::new("/a.txt").to_path_buf()],
        "no-provider-or-saved-server-knows-this".to_string(),
        "/photos".to_string(),
        VolumeCopyConfig::default(),
        Initiator::User,
        None,
    )
    .await
    .expect_err("a copy onto an unknown volume is refused");
    get_volume_manager().unregister(&source_id);

    assert!(
        matches!(refused, WriteOperationError::IoError { .. }),
        "an unknown id is a missing volume; got {refused:?}"
    );
}

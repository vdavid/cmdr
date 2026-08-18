//! The two resolution questions every transfer asks before it starts.
//!
//! Both answers are load-bearing and neither is obvious from its inputs: an
//! unanchored destination reaches a share as a path outside its mount, and a
//! `.zip` FILE routed to its own `ArchiveVolume` would scan the archive's contents
//! instead of copying the file.

use super::{resolve_dest_path, resolve_source_volume};
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
    let (vol, is_inside) = resolve_source_volume("root", Some(&zip)).await.expect("source volume");
    assert!(!is_inside, "the .zip file itself is not archive-inner");
    assert_eq!(vol.name(), "Root", "routed to the parent volume, not the archive");

    // A path INSIDE the archive routes to the ArchiveVolume, is_inside = true.
    // This is what makes an EXTRACT reachable as an ordinary copy.
    let (inner_vol, inner_is_inside) = resolve_source_volume("root", Some(&zip.join("entry.txt")))
        .await
        .expect("inner volume");
    assert!(inner_is_inside, "an inner path is archive-inner");
    assert_eq!(inner_vol.root(), zip, "the archive volume's root is the .zip");
}

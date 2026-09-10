//! The rules both saved-server stores share: where a start folder may sit.

use super::*;

#[test]
fn no_start_folder_means_the_root() {
    assert_eq!(start_folder_under_root("/srv/data", None), Ok(None));
    assert_eq!(start_folder_under_root("/srv/data", Some("")), Ok(None));
    assert_eq!(start_folder_under_root("/srv/data", Some("   ")), Ok(None));
}

/// The root itself, however it's spelled, is stored as no start folder at all,
/// so one landing has one spelling.
#[test]
fn a_start_folder_at_the_root_is_stored_as_none() {
    assert_eq!(start_folder_under_root("/srv/data", Some("/srv/data")), Ok(None));
    assert_eq!(start_folder_under_root("/srv/data/", Some("/srv/./data")), Ok(None));
    assert_eq!(start_folder_under_root("/", Some("/")), Ok(None));
}

/// The prod case: a root widened to `naspi`, landing where it used to be rooted.
#[test]
fn a_start_folder_under_the_root_is_stored_normalized() {
    assert_eq!(
        start_folder_under_root("/share/ZFS18_DATA/naspi", Some("/share/ZFS18_DATA/naspi/tmp/")),
        Ok(Some("/share/ZFS18_DATA/naspi/tmp".to_string()))
    );
    assert_eq!(
        start_folder_under_root("/srv/data", Some("/srv/data/photos/../docs")),
        Ok(Some("/srv/data/docs".to_string()))
    );
    assert_eq!(
        start_folder_under_root("/", Some("/home/ada")),
        Ok(Some("/home/ada".to_string()))
    );
}

/// ❗ The trap a string prefix falls into: a sibling whose name merely starts
/// with the root's.
#[test]
fn a_sibling_that_spells_the_roots_prefix_is_outside_it() {
    assert_eq!(
        start_folder_under_root("/srv/data", Some("/srv/data-1/photos")),
        Err(StartFolderOutsideRoot)
    );
}

#[test]
fn a_start_folder_above_the_root_or_escaping_it_is_outside() {
    assert_eq!(
        start_folder_under_root("/srv/data", Some("/srv")),
        Err(StartFolderOutsideRoot)
    );
    assert_eq!(
        start_folder_under_root("/srv/data", Some("/srv/data/../etc")),
        Err(StartFolderOutsideRoot)
    );
}

/// A connect carrying a saved start folder across a root that no longer holds it
/// lands at the root, ❌ never saves a place no pane could reach.
#[test]
fn a_connect_drops_a_saved_start_folder_the_root_no_longer_holds() {
    assert_eq!(
        start_folder_for_root("/srv/data", Some("/srv/data/photos".to_string())),
        Some("/srv/data/photos".to_string())
    );
    assert_eq!(
        start_folder_for_root("/srv/other", Some("/srv/data/photos".to_string())),
        None
    );
    assert_eq!(start_folder_for_root("/srv/data", None), None);
}

/// A relative start folder is read from `/`, the way `RemoteRoot` reads a
/// relative root, ❌ never from the root: the field says an absolute server path.
#[test]
fn a_relative_start_folder_is_read_from_the_server_root() {
    assert_eq!(
        start_folder_under_root("/srv/data", Some("photos")),
        Err(StartFolderOutsideRoot)
    );
    assert_eq!(
        start_folder_under_root("/", Some("photos")),
        Ok(Some("/photos".to_string()))
    );
}

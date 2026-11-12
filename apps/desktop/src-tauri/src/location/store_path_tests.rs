//! The importance store's path space against the app paths every caller holds.

use super::*;

const SHARE: &str = "/Volumes/testshare-1";
const DRIVE: &str = "/Users/test/Library/CloudStorage/GoogleDrive-test@example.com";

#[test]
fn a_share_path_loses_its_mount_point() {
    assert_eq!(
        to_store_path("/Volumes/testshare-1/documents/x", SHARE).as_deref(),
        Some("/documents/x")
    );
    assert_eq!(to_store_path(SHARE, SHARE).as_deref(), Some("/"), "the root itself");
}

#[test]
fn a_cloud_drive_path_is_relative_to_the_drive_not_the_boot_disk() {
    let shared = format!("{DRIVE}/Apps");
    assert_eq!(to_store_path(&shared, DRIVE).as_deref(), Some("/Apps"));
}

#[test]
fn the_boot_disk_keeps_absolute_paths() {
    assert_eq!(
        to_store_path("/Users/test/Projects", "/").as_deref(),
        Some("/Users/test/Projects")
    );
}

#[test]
fn a_path_off_the_volume_has_no_store_path() {
    assert_eq!(
        to_store_path("/Volumes/testshare/documents", SHARE),
        None,
        "a sibling mount"
    );
    assert_eq!(
        to_store_path("/Volumes/testshare-10/documents", SHARE),
        None,
        "whole segments only"
    );
}

#[test]
fn a_store_row_joins_back_onto_its_root() {
    assert_eq!(
        from_store_path("/documents/x", SHARE).as_deref(),
        Some("/Volumes/testshare-1/documents/x")
    );
    assert_eq!(from_store_path("/", SHARE).as_deref(), Some(SHARE));
    assert_eq!(
        from_store_path("/Users/test/Projects", "/").as_deref(),
        Some("/Users/test/Projects")
    );
}

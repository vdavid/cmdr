//! The symlink seam, and how the copy scan counts a link.

use super::*;
use cmdr_fs::testing::TestDir;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{Volume, VolumeError};
use std::path::Path;

/// A copy's totals: a link inside a folder is one entry worth no bytes, as the
/// shared scan and the preview count it, ❌ never skipped and ❌ never followed.
#[tokio::test]
async fn test_scan_for_copy_counts_a_nested_link_as_one_entry_without_following_it() {
    let test_dir = TestDir::new("scan_counts_link");
    std::fs::create_dir_all(test_dir.join("real")).unwrap();
    std::fs::write(test_dir.join("real/big.bin"), vec![0u8; 4096]).unwrap();
    std::fs::create_dir_all(test_dir.join("tree")).unwrap();
    std::fs::write(test_dir.join("tree/keep.txt"), b"kept").unwrap();
    cmdr_fs::testing::symlink("../real", test_dir.join("tree/link")).unwrap();
    let volume = LocalPosixVolume::local_folder("Test", test_dir.to_path_buf(), VolumeHost::detached());

    let scan = volume.scan_for_copy(Path::new("/tree")).await.unwrap();

    assert_eq!(scan.file_count, 2, "the file and the link");
    assert_eq!(scan.total_bytes, 4, "the file's bytes alone");
}

/// A filesystem that can't hold a link (the error `exFAT` answers with) reads as
/// exactly that, and a missing folder as the plain failure it is.
#[tokio::test]
async fn test_read_link_target_of_something_missing_is_not_found() {
    let test_dir = TestDir::new("read_link_missing");
    let volume = LocalPosixVolume::local_folder("Test", test_dir.to_path_buf(), VolumeHost::detached());
    assert!(matches!(
        volume.read_link_target(Path::new("/nope")).await,
        Err(VolumeError::NotFound(_))
    ));
    assert!(matches!(
        volume.create_symlink(Path::new("/nope/link"), Path::new("t")).await,
        Err(VolumeError::NotFound(_))
    ));
}

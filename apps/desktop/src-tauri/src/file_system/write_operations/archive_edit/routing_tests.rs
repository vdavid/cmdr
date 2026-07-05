//! Tests for the shared routing primitives: the zip-only write guard and the
//! duplicate-existence oracle (including the REMOTE-parent path, which must read
//! through the parent volume, not attempt a local file open).

use super::test_support::*;
use super::*;

#[tokio::test]
async fn archive_inner_exists_detects_a_duplicate_in_a_remote_archive() {
    // The mkdir/mkfile duplicate pre-check must see entries inside a REMOTE archive
    // (through the parent volume), not fail open by attempting a local file open.
    let archive_path = PathBuf::from("/device/bundle.zip");
    let (parent_id, _parent) = register_remote_zip(&archive_path, &[("existing.txt", b"x")]).await;

    assert!(
        archive_inner_exists(&parent_id, &archive_path, "existing.txt").await,
        "a duplicate inside a remote archive must be detected"
    );
    assert!(
        !archive_inner_exists(&parent_id, &archive_path, "not_there.txt").await,
        "a non-existent inner path reports absent"
    );

    get_volume_manager().unregister(&parent_id);
}

/// The mutation refusal matrix: only zip is writable. Every non-zip archive
/// format (tar family + 7z) refuses with a typed `ReadOnlyDevice` at the write
/// chokepoint, so no archive-edit route ever hands a non-zip file to the
/// zip-only mutator. Path-only (extension-based), no I/O.
#[test]
fn ensure_zip_writable_allows_zip_and_refuses_read_only_formats() {
    use std::path::Path;
    assert!(ensure_zip_writable(Path::new("/x/writable.zip")).is_ok());
    for name in [
        "ro.tar",
        "ro.tar.gz",
        "ro.tgz",
        "ro.tar.xz",
        "ro.txz",
        "ro.tar.bz2",
        "ro.tbz2",
        "ro.tar.zst",
        "ro.tzst",
        "ro.7z",
    ] {
        let path = format!("/x/{name}");
        let err = ensure_zip_writable(Path::new(&path)).expect_err(name);
        assert!(
            matches!(err, WriteOperationError::ReadOnlyDevice { .. }),
            "{name}: {err:?}"
        );
    }
}

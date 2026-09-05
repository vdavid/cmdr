//! Tests for the shared routing primitives: the zip-only write guard and the
//! duplicate-existence oracle (including the REMOTE-parent path, which must read
//! through the parent volume, not attempt a local file open).

use super::test_support::*;
use super::*;
use crate::file_system::write_operations::types::ReadOnlySide;

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

/// The guard carries the caller's side through, because a tar has the same two
/// directions a `.git` snapshot does: you can copy OUT of one but not move out
/// (the source can't delete the original), and you can't write INTO one at all.
/// Wording the first case as "choose a different destination" would name the
/// half that was fine.
#[test]
fn ensure_zip_writable_names_the_half_the_caller_was_looking_at() {
    use std::path::Path;

    let moving_out =
        ensure_zip_writable(Path::new("/x/ro.tar"), ReadOnlySide::Source).expect_err("a move out of a tar is refused");
    assert!(
        matches!(
            moving_out,
            WriteOperationError::ReadOnlyDevice {
                side: ReadOnlySide::Source,
                ..
            }
        ),
        "{moving_out:?}"
    );

    let writing_in = ensure_zip_writable(Path::new("/x/ro.tar"), ReadOnlySide::Destination)
        .expect_err("a write into a tar is refused");
    assert!(
        matches!(
            writing_in,
            WriteOperationError::ReadOnlyDevice {
                side: ReadOnlySide::Destination,
                ..
            }
        ),
        "{writing_in:?}"
    );
}

/// The mutation refusal matrix: only zip is writable. Every non-zip archive
/// format (tar family + 7z) refuses with a typed `ReadOnlyDevice` at the write
/// chokepoint, so no archive-edit route ever hands a non-zip file to the
/// zip-only mutator. Path-only (extension-based), no I/O.
#[test]
fn ensure_zip_writable_allows_zip_and_refuses_read_only_formats() {
    use std::path::Path;
    assert!(ensure_zip_writable(Path::new("/x/writable.zip"), ReadOnlySide::Destination).is_ok());
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
        let err = ensure_zip_writable(Path::new(&path), ReadOnlySide::Destination).expect_err(name);
        assert!(
            matches!(err, WriteOperationError::ReadOnlyDevice { .. }),
            "{name}: {err:?}"
        );
    }
}

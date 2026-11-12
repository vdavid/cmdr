//! The local write fast path's restic guard: the view of a confirmed repo is
//! refused on both sides of a transfer, and a real folder that merely shares
//! the row's name (or a real folder named like an archive) still takes an
//! ordinary local copy.

use super::{local_transfer_request, reject_if_routed_over_a_parent};
use cmdr_operations::{ReadOnlySide, WriteOperationError};

#[test]
fn a_restic_view_is_refused_but_a_lookalike_folder_is_not() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path().join("test-restic");
    for folder in ["data", "index", "keys", "snapshots"] {
        std::fs::create_dir_all(repo.join(folder)).expect("repo folder");
    }
    std::fs::write(repo.join("config"), b"encrypted").expect("config");
    let view_file = repo.join("Browse snapshots/latest/Users/me/a.txt");

    for side in [ReadOnlySide::Source, ReadOnlySide::Destination] {
        let err = reject_if_routed_over_a_parent(std::iter::once(&view_file), side)
            .expect_err("a path in the view is refused");
        assert!(matches!(err, WriteOperationError::ReadOnlyDevice { .. }), "{err:?}");
    }
    assert!(
        reject_if_routed_over_a_parent(std::iter::once(&repo.join("config")), ReadOnlySide::Source).is_ok(),
        "the repo's real files are ordinary files"
    );

    let lookalike = dir.path().join("photos/Browse snapshots");
    std::fs::create_dir_all(&lookalike).expect("a real folder of that name");
    let zip_named = dir.path().join("photos.zip");
    std::fs::create_dir_all(&zip_named).expect("a real folder named like an archive");
    for destination in [&lookalike, &zip_named] {
        assert!(
            local_transfer_request(&[], &destination.to_string_lossy(), None).is_ok(),
            "a copy into {} stays on the fast path",
            destination.display()
        );
    }
}

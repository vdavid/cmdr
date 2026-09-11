//! Rolling back onto a remote place whose root was narrowed after the operation
//! ran: what counts as already gone, and what fails instead
//! (`../DETAILS.md` § "What counts as already gone").
//!
//! `RootFenced` stands in for the narrowed volume: it refuses every path above its
//! root as `NotFound`, the way `SftpVolume` and `WebdavVolume` do, while the files
//! stay on the "server" behind it.

use std::sync::Arc;

use super::test_support::*;
use super::*;
use crate::file_system::volume::InMemoryVolume;

/// A server holding `/share/naspi` with `tmp/` and `other/` folders.
async fn nas_server() -> Arc<InMemoryVolume> {
    let server = Arc::new(InMemoryVolume::new("NAS"));
    for dir in ["/share", "/share/naspi", "/share/naspi/tmp", "/share/naspi/other"] {
        mkdir(&server, dir).await;
    }
    server
}

fn every_skip_failed(report: &RollbackReport) -> bool {
    report
        .skips
        .iter()
        .all(|group| matches!(group.reason, SkipReason::Failed))
}

/// The reported shape, seen on a real NAS: copy into `/share/naspi/other`, narrow
/// the place's root to `/share/naspi/tmp`, then undo the copy. The narrowed
/// volume refuses the copy's paths as `NotFound`, and ❗ that is NOT the files
/// being gone: they're still on the server. Counting them as already gone reported
/// a clean undo that removed nothing, so they fail safe instead.
#[tokio::test]
async fn an_item_a_narrowed_root_no_longer_reaches_fails_instead_of_counting_as_gone() {
    let rig = Rig::new();
    let server = nas_server().await;
    put(&server, "/share/naspi/other/a.txt", b"aaa").await;
    mkdir(&server, "/share/naspi/other/made").await;
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.vm.register("nas", RootFenced::over(&server, "/share/naspi/tmp"));
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("nas"),
        RollbackState::Rollbackable,
        vec![
            file_unit(0, "src", "/a.txt", "nas", "/share/naspi/other/a.txt", 3),
            dir_unit(1, "nas", "/share/naspi/other/made"),
        ],
    );

    let report = rig.rollback("op").await;

    assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);
    assert_eq!((report.reversed, report.skipped), (0, 2));
    assert!(every_skip_failed(&report), "{:?}", report.skips);
    assert!(
        exists(&server, "/share/naspi/other/a.txt").await,
        "the copy is still there"
    );
    assert!(exists(&server, "/share/naspi/other/made").await);
}

/// A restore the narrowed root cuts off on EITHER side: the item to move back
/// sits outside it (a rename in `other/`), or the place to put it back does (a
/// move from `other/` into `tmp/`). Both stay where the operation left them.
#[tokio::test]
async fn a_restore_a_narrowed_root_cuts_off_on_either_side_fails_instead_of_counting_as_gone() {
    let rig = Rig::new();
    let server = nas_server().await;
    put(&server, "/share/naspi/other/new.txt", b"renamed").await;
    put(&server, "/share/naspi/tmp/moved.txt", b"moved").await;
    rig.vm.register("nas", RootFenced::over(&server, "/share/naspi/tmp"));
    rig.seed(
        "op",
        OpKind::Move,
        "nas",
        Some("nas"),
        RollbackState::Rollbackable,
        vec![
            file_unit(
                0,
                "nas",
                "/share/naspi/other/old.txt",
                "nas",
                "/share/naspi/other/new.txt",
                7,
            ),
            file_unit(
                1,
                "nas",
                "/share/naspi/other/moved.txt",
                "nas",
                "/share/naspi/tmp/moved.txt",
                5,
            ),
        ],
    );

    let report = rig.rollback("op").await;

    assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);
    assert_eq!((report.reversed, report.skipped), (0, 2));
    assert!(every_skip_failed(&report), "{:?}", report.skips);
    assert!(exists(&server, "/share/naspi/other/new.txt").await);
    assert!(exists(&server, "/share/naspi/tmp/moved.txt").await);
}

/// The other half: an item missing INSIDE the narrowed root really is gone, and
/// stays the idempotent no-op it always was.
#[tokio::test]
async fn an_item_missing_inside_a_narrowed_root_still_counts_as_already_gone() {
    let rig = Rig::new();
    let server = nas_server().await;
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.vm.register("nas", RootFenced::over(&server, "/share/naspi/tmp"));
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("nas"),
        RollbackState::Rollbackable,
        vec![
            file_unit(0, "src", "/a.txt", "nas", "/share/naspi/tmp/a.txt", 3),
            dir_unit(1, "nas", "/share/naspi/tmp/made"),
        ],
    );

    let report = rig.rollback("op").await;

    assert_eq!(report.final_state, RollbackState::RolledBack);
    assert_eq!((report.reversed, report.skipped), (2, 0));
}

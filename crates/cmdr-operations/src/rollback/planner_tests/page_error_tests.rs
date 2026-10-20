//! A rollback whose journal stops answering partway (a page read fails: the database
//! stays busy past its timeout, a row can't be decoded, an I/O error) ends like a
//! stop: it keeps what it reversed, the op stays retryable, and the inverse op is
//! marked `Failed`. It never ends as a complete undo.

use std::sync::Arc;

use super::test_support::*;
use cmdr_fs::volume::InMemoryVolume;
use cmdr_operation_log::store::{open_read_connection, read_inverse_op};
use cmdr_operation_log::types::{ExecutionStatus, OpKind, RollbackState};

/// More file rows than two pages (512 each) plus one created folder, newest last, so
/// the first page holds the folder and 511 files.
const FILES: i64 = 1_200;

async fn copied_files_and_a_folder() -> (Rig, Arc<InMemoryVolume>) {
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    let mut units = Vec::new();
    for i in 0..FILES {
        let p = format!("/f{i}.bin");
        put(&dst, &p, b"x").await;
        units.push(file_unit(i, "src", &p, "dst", &p, 1));
    }
    mkdir(&dst, "/made").await;
    units.push(dir_unit(FILES, "dst", "/made"));
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", dst.clone());
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        units,
    );
    (rig, dst)
}

/// Make one row unreadable, the way a token this build doesn't know would be.
fn corrupt_row(rig: &Rig, seq: i64) {
    let conn = cmdr_fs::sqlite_util::open(rig.writer.db_path()).expect("open db");
    let changed = conn
        .execute(
            "UPDATE operation_items SET entry_type = 'from_a_newer_build' WHERE op_id = 'op' AND seq = ?1",
            [seq],
        )
        .expect("corrupt row");
    assert_eq!(changed, 1);
}

fn inverse_status(rig: &Rig) -> ExecutionStatus {
    let conn = open_read_connection(rig.writer.db_path()).expect("read conn");
    read_inverse_op(&conn, "op")
        .expect("read inverse")
        .expect("inverse present")
        .execution_status
}

#[tokio::test]
async fn a_page_that_fails_to_read_ends_the_run_partial_never_rolled_back() {
    let (rig, dst) = copied_files_and_a_folder().await;
    corrupt_row(&rig, 400); // on the second page

    let report = rig.rollback("op").await;

    assert_eq!(report.reversed, 511, "the first page's files came back: {report:?}");
    assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);
    assert_eq!(rig.read_op("op").rollback_state, RollbackState::PartiallyRolledBack);
    assert_eq!(inverse_status(&rig), ExecutionStatus::Failed);
    assert!(!exists(&dst, &format!("/f{}.bin", FILES - 1)).await);
    assert!(
        exists(&dst, "/f0.bin").await,
        "nothing past the unreadable page is touched"
    );
    assert!(
        exists(&dst, "/made").await,
        "the folder sweep doesn't run over a tree the run never finished reading"
    );
}

#[tokio::test]
async fn a_first_page_that_fails_to_read_leaves_the_op_a_clean_retry() {
    let (rig, dst) = copied_files_and_a_folder().await;
    corrupt_row(&rig, FILES - 1);

    let report = rig.rollback("op").await;

    assert_eq!((report.reversed, report.skipped), (0, 0), "{report:?}");
    assert_eq!(report.final_state, RollbackState::Rollbackable);
    assert_eq!(rig.read_op("op").rollback_state, RollbackState::Rollbackable);
    assert_eq!(inverse_status(&rig), ExecutionStatus::Failed);
    assert!(exists(&dst, "/made").await);
}

//! The directory sweep: a rollback removes the folders an operation created only
//! after EVERY file row is through (across pages, not per page), and deepest path
//! first, whatever order their `seq`s put them in the stream.

use std::sync::Arc;

use super::test_support::*;
use cmdr_fs::volume::InMemoryVolume;
use cmdr_operation_log::types::{OpKind, RollbackState};

/// Files straight under `/tree`, enough that the stream spans two pages (512 rows
/// each) and the file inside `/tree/sub` lands on the second one.
const FILES: i64 = 600;

#[tokio::test]
async fn created_folders_go_deepest_first_after_every_file_even_when_streamed_first() {
    let rig = Rig::new();
    let src = Arc::new(InMemoryVolume::new("Src"));
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    mkdir(&dst, "/tree").await;
    mkdir(&dst, "/tree/sub").await;
    // Lowest seq, so it streams last: the second page.
    let mut units = vec![file_unit(
        0,
        "src",
        "/tree/sub/deep.txt",
        "dst",
        "/tree/sub/deep.txt",
        4,
    )];
    put(&dst, "/tree/sub/deep.txt", b"deep").await;
    for i in 1..=FILES {
        let p = format!("/tree/f{i}.txt");
        put(&dst, &p, b"x").await;
        units.push(file_unit(i, "src", &p, "dst", &p, 1));
    }
    // The folders' rows carry the HIGHEST seqs, the shallower one highest of all, so
    // `seq DESC` streams `/tree`, then `/tree/sub`, before any file. Only the sweep
    // (deferred past the last page, sorted deepest first) can empty them in order.
    units.push(dir_unit(FILES + 1, "dst", "/tree/sub"));
    units.push(dir_unit(FILES + 2, "dst", "/tree"));
    rig.register("src", src);
    rig.register("dst", dst.clone());
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        units,
    );
    rig.put_originals("op").await;

    let report = rig.rollback("op").await;

    assert_eq!(
        report.skipped, 0,
        "no folder was tried while it held a file: {report:?}"
    );
    assert_eq!(report.reversed, FILES as u64 + 3);
    assert_eq!(report.final_state, RollbackState::RolledBack);
    assert!(!exists(&dst, "/tree/sub/deep.txt").await);
    assert!(!exists(&dst, "/tree/sub").await, "the deeper folder went first");
    assert!(!exists(&dst, "/tree").await, "then the folder that held it");
}

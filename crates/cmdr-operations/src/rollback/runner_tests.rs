//! The reversal runner's own cells: which progress frames go out, and the acts it
//! performs.

use super::*;
use crate::testing::TestOperations;

/// Two rules govern which frames go out, and both matter: the throttle keeps a
/// million-item reversal from emitting an event per item, and the "landed on
/// the total" exemption keeps a throttled run from ending on a stale frame that
/// says 1 of 3.
#[test]
fn the_throttle_holds_frames_back_but_never_the_one_that_lands_on_the_total() {
    let engine = TestOperations::new();
    let reversal = Reversal::emitting_every(&engine, "progress-throttle", Duration::from_secs(600));
    let runner = reversal.runner();
    let report = |files_done: u64| RollbackProgress {
        files_done,
        files_total: 3,
        bytes_done: files_done,
        bytes_total: 3,
        current_name: None,
    };

    runner.report_progress(report(0));
    runner.report_progress(report(1));
    runner.report_progress(report(2));
    assert_eq!(
        reversal.frames().len(),
        1,
        "the first frame goes out at once; the interval holds the rest back"
    );

    runner.report_progress(report(3));
    let frames = reversal.frames();
    assert_eq!(frames.len(), 2, "the frame that reaches the total ignores the throttle");
    assert_eq!(frames[1].files_done, 3);
    assert_eq!(frames[1].phase, WriteOperationPhase::RollingBack);
}

/// ❗ Undoing a move on an object store, where a big file's rename is a copy
/// (`Volume::rename_work`) and `rename` refuses it: the file goes back
/// copied and then deleted, never through the `rename` that would fail.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_same_volume_restore_that_renames_by_copy_goes_back_by_copy() {
    use cmdr_fs::volume::{InMemoryVolume, Volume};
    use std::path::Path;

    let volume: Arc<dyn Volume> = Arc::new(
        InMemoryVolume::new("Bucket")
            .with_whole_publish()
            .with_renames_by_copy()
            .with_space_info(1_000_000, 1_000_000),
    );
    volume.create_directory(Path::new("/moved")).await.unwrap();
    volume
        .create_file(Path::new("/moved/big.mov"), b"frames")
        .await
        .unwrap();
    let engine = TestOperations::new();
    let reversal = Reversal::on(&engine, "restore-by-copy");

    reversal
        .runner()
        .perform(InverseAct::Restore {
            from: &volume,
            from_path: Path::new("/moved/big.mov"),
            to: &volume,
            to_path: Path::new("/home/big.mov"),
            same_volume: true,
            force: false,
        })
        .await
        .expect("the restore lands");

    assert!(volume.exists(Path::new("/home/big.mov")).await, "back where it started");
    assert!(
        !volume.exists(Path::new("/moved/big.mov")).await,
        "and gone from where it was"
    );
}

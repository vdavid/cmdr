//! The directory sweep: a rollback removes the folders an operation created only
//! after EVERY file row is through (across pages, not per page), and deepest path
//! first, whatever order their `seq`s put them in the stream; and what a sweep
//! leaves durable when it's stopped or dropped inside.

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::test_support::*;
use cmdr_fs::volume::{InMemoryVolume, VolumeError};
use cmdr_operation_log::rollback::{ActRefusal, InverseAct, RollbackProgress, RollbackRunner, execute_rollback};
use cmdr_operation_log::types::{Initiator, ItemOutcome, OpKind, RollbackState};

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

/// More folder units than one journal page (the engine's `ROLLBACK_PAGE` is 512),
/// so a sweep that flushes per page has made one flush before the run is dropped.
const SWEEP_DIRS: usize = 600;

/// The folder the sweep hangs on: past the first page, short of the last.
const HANG_AT: usize = 550;

/// Removes what it's told to, then parks FOREVER at the item boundary once
/// `HANG_AT` folders are gone, signalling `parked` first. Dropping the run there
/// is what "pause, then quit Cmdr" does to a live sweep.
struct HangMidSweep {
    dirs_removed: AtomicUsize,
    parked: tokio::sync::Notify,
}

impl RollbackRunner for HangMidSweep {
    fn perform<'a>(&'a self, act: InverseAct<'a>) -> Pin<Box<dyn Future<Output = Result<(), ActRefusal>> + Send + 'a>> {
        Box::pin(async move {
            match act {
                InverseAct::RemoveDir { volume, path } => {
                    volume.delete(path).await?;
                    self.dirs_removed.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
                _ => Err(VolumeError::NotSupported.into()),
            }
        })
    }

    fn should_stop(&self) -> bool {
        false
    }

    fn wait_while_paused(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        if self.dirs_removed.load(Ordering::SeqCst) >= HANG_AT {
            self.parked.notify_one();
            Box::pin(std::future::pending())
        } else {
            Box::pin(std::future::ready(()))
        }
    }

    fn report_progress(&self, _progress: RollbackProgress<'_>) {}
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sweep_dropped_midway_leaves_the_op_partly_rolled_back_after_relaunch() {
    // Regression: the sweep flushed once, after its last folder, so a run dropped
    // inside it left no durable outcome, and the next launch's reconcile reset an op
    // with hundreds of folders already back to `rollbackable`.
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    let mut units = Vec::new();
    for i in 0..SWEEP_DIRS {
        let path = format!("/d{i}");
        mkdir(&dst, &path).await;
        units.push(dir_unit(i as i64, "dst", &path));
    }
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", Arc::clone(&dst));
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        units,
    );
    // What the gate leaves behind when it lets a run start.
    rig.writer
        .set_rollback_state("op", RollbackState::RollingBack, None)
        .expect("flip to rolling back");

    let runner = HangMidSweep {
        dirs_removed: AtomicUsize::new(0),
        parked: tokio::sync::Notify::new(),
    };
    let original = rig.read_op("op");
    tokio::select! {
        _ = execute_rollback(&rig.vm, &rig.writer, &original, "inv-1", Initiator::User, &runner) => {
            panic!("the runner parks forever, so the run can't finish");
        }
        () = runner.parked.notified() => {}
    }

    let rig = rig.relaunched();
    assert_eq!(
        rig.read_op("op").rollback_state,
        RollbackState::PartiallyRolledBack,
        "the folders the sweep already put back reach the journal before the run ends"
    );
}

/// Turns the stop on from INSIDE the first folder's act and refuses it, the shape
/// a reversal that observes a cancel mid-item takes.
struct StopInsideTheAct {
    stopped: AtomicBool,
}

impl RollbackRunner for StopInsideTheAct {
    fn perform<'a>(
        &'a self,
        _act: InverseAct<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ActRefusal>> + Send + 'a>> {
        self.stopped.store(true, Ordering::SeqCst);
        Box::pin(std::future::ready(Err(ActRefusal::Locked)))
    }

    fn should_stop(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    fn wait_while_paused(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(std::future::ready(()))
    }

    fn report_progress(&self, _progress: RollbackProgress<'_>) {}
}

#[tokio::test]
async fn a_stop_inside_a_sweep_item_records_no_skip() {
    // Mirrors the file loop: a refusal that coincides with the stop IS the stop, not
    // a held-back folder with a stored skip reason the tree would show.
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    mkdir(&dst, "/only").await;
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", Arc::clone(&dst));
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        vec![dir_unit(0, "dst", "/only")],
    );

    let runner = StopInsideTheAct {
        stopped: AtomicBool::new(false),
    };
    let original = rig.read_op("op");
    let report = execute_rollback(&rig.vm, &rig.writer, &original, "inv-1", Initiator::User, &runner).await;

    assert!(report.canceled, "the stop is observed");
    assert_eq!(report.skipped, 0, "nothing counts as held back: {report:?}");
    let row = &rig.read_items("op")[0];
    assert_eq!(row.rollback_skip_reason, None, "no skip reason is stored");
    assert_ne!(row.outcome, ItemOutcome::Skipped);
}

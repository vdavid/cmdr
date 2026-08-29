//! What a reversal does while it's RUNNING: how it reports, how it parks, and
//! what it leaves behind when it's stopped inside one large file.
//!
//! `tests.rs` pins what a reversal decides (the per-kind inverse, the snapshot
//! recheck, the never-overwrite restore, the typed skip reasons). These pin the
//! live behavior around those decisions, which is the pair
//! `execute_rollback` + `write_operations::rollback::ReversalRunner` working
//! together.
//!
//! **Timing comes from the machinery, never from a sleep.** A reversal of
//! in-memory volumes is over inside one poll, so a test about a reversal IN
//! PROGRESS would be racing it. Two hooks open a real window: `InMemoryVolume`'s
//! `with_read_chunk_delay` (each read chunk takes time, so a restore spends
//! measurable time INSIDE one file), and `test_mode::pace_rollback_for_test` (the
//! E2E per-item throttle, for a reversal whose items are deletes and stream
//! nothing). The waits are still on STATE (a frame that arrived, bytes that
//! moved), never on a clock.
//!
//! **Cancel and pause go through the public calls**, never the intent atom:
//! `Reversal::stop` / `pause` / `resume` are `cancel_write_operation` and the
//! live half of `pause_operation` / `resume_operation`, by operation id.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use super::test_support::*;
use super::{InverseAct, RollbackProgress, RollbackRunner, execute_rollback, rollback_operation};
use crate::file_system::volume::{InMemoryVolume, Volume, VolumeError};
use crate::file_system::write_operations::rollback::Reversal;
use crate::operation_log::types::{Initiator, OpKind, RollbackState};
use crate::test_support::wait_until_async;

/// Long enough that a six-item restore can't outrun a poll, short enough that the
/// suite stays in the tens of milliseconds.
const CHUNK_DELAY: Duration = Duration::from_millis(15);

/// The per-item pace for a reversal that streams nothing, so a pause has an item
/// boundary to land on.
const ITEM_PACE_MS: u64 = 15;

/// How long a "it isn't moving" window runs. A generous multiple of the per-item
/// pace, so an unparked reversal would have finished several items inside it.
const HELD_WINDOW: Duration = Duration::from_millis(ITEM_PACE_MS * 5);

const PATIENCE: Duration = Duration::from_secs(10);

/// The bytes of one seeded file. One chunk each, so the per-chunk delay is also
/// a per-ITEM delay.
const FILE_BYTES: usize = 32;

/// A rig whose files sit on `dst` as the copies a COPY left there: reversing it
/// is a run of deletes, which stream nothing.
async fn copied_onto(count: usize) -> (Rig, Arc<InMemoryVolume>) {
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    let mut units = Vec::new();
    for i in 0..count {
        let path = format!("/f{i}.txt");
        put(&dst, &path, &[b'x'; FILE_BYTES]).await;
        units.push(file_unit(i as i64, "src", &path, "dst", &path, FILE_BYTES as i64));
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
    (rig, dst)
}

/// How many of `count` copies the reversal has removed.
async fn removed(dst: &InMemoryVolume, count: usize) -> usize {
    let mut gone = 0;
    for i in 0..count {
        if !exists(dst, &format!("/f{i}.txt")).await {
            gone += 1;
        }
    }
    gone
}

/// A rig whose files sit on `dst` and belong back on `src`: the state a
/// cross-volume MOVE leaves behind, and the one whose reversal streams bytes.
async fn moved_across(count: usize, chunk_delay: Duration) -> (Rig, Arc<InMemoryVolume>, Arc<InMemoryVolume>) {
    let rig = Rig::new();
    let src = Arc::new(InMemoryVolume::new("Src"));
    let dst = Arc::new(InMemoryVolume::new("Dst").with_read_chunk_delay(chunk_delay));
    let mut units = Vec::new();
    for i in 0..count {
        let path = format!("/f{i}.txt");
        put(&dst, &path, &[b'x'; FILE_BYTES]).await;
        units.push(file_unit(i as i64, "src", &path, "dst", &path, FILE_BYTES as i64));
    }
    rig.register("src", Arc::clone(&src));
    rig.register("dst", Arc::clone(&dst));
    rig.seed(
        "op",
        OpKind::Move,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        units,
    );
    (rig, src, dst)
}

/// How many of `count` files have made it back to `src`.
async fn restored(src: &InMemoryVolume, count: usize) -> usize {
    let mut back = 0;
    for i in 0..count {
        if exists(src, &format!("/f{i}.txt")).await {
            back += 1;
        }
    }
    back
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn progress_runs_forward_from_the_journal_totals_and_reaches_them() {
    // A reversal launched from history opens a FRESH bar, so it counts UP. (The
    // in-flight rollback of a cancelled copy drains a bar that's already full;
    // that's a different bar, and it stays as it is.)
    let count = 4;
    let (rig, src, _dst) = moved_across(count, Duration::ZERO).await;
    let reversal = Reversal::new("rollback-progress");

    let report = rig.rollback_driven_by("op", "inv-1", &reversal).await;
    assert_eq!(report.reversed, count as u64);
    assert_eq!(restored(&src, count).await, count);

    let frames = reversal.frames();
    assert!(!frames.is_empty(), "a reversal must report progress");
    // No scanning phase: the totals come off the journal, so the FIRST frame
    // already knows how big the job is.
    let first = &frames[0];
    assert_eq!(first.files_total, count, "the first frame already carries the total");
    assert_eq!(first.bytes_total, (count * FILE_BYTES) as u64);
    assert_eq!(first.files_done, 0, "and starts at nothing done");

    for pair in frames.windows(2) {
        assert!(
            pair[1].files_done >= pair[0].files_done && pair[1].bytes_done >= pair[0].bytes_done,
            "progress must never go backwards: {} then {}",
            pair[0].files_done,
            pair[1].files_done
        );
    }

    let last = frames.last().expect("frames is not empty");
    assert_eq!(last.files_done, count, "the bar ends where the work ended");
    assert_eq!(last.bytes_done, (count * FILE_BYTES) as u64);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_paused_reversal_stops_advancing_and_resumes_where_it_left_off() {
    // A COPY undo on purpose: every item is a delete, so the loop's own pause gate
    // is the ONLY thing that can park it — nothing streams, so the streaming
    // layer's between-chunks checkpoint never gets a say. That's exactly the shape
    // the history dialog's Roll back takes, and the shape a missing item-boundary
    // gate would run straight through.
    let count = 6;
    let (rig, dst) = copied_onto(count).await;
    let _pacing = crate::test_mode::pace_rollback_for_test(ITEM_PACE_MS).await;
    let reversal = Reversal::new("rollback-pause");

    let (report, ()) = tokio::join!(rig.rollback_driven_by("op", "inv-1", &reversal), async {
        // Wait for the reversal to be genuinely under way, so the pause lands in
        // the middle rather than before the first item.
        wait_until_async(PATIENCE, "the reversal to remove its first copy", || {
            reversal.frames().last().is_some_and(|frame| frame.files_done >= 1)
        })
        .await;
        reversal.pause();

        // The item already past the gate runs to its end; the pause takes hold at
        // the NEXT boundary, like every other driver's does. Give that one item its
        // pace before sampling, so what's measured is the park and not the item the
        // click landed inside.
        // allowed-test-sleep: letting the in-flight item finish; nothing signals "the last act returned".
        tokio::time::sleep(Duration::from_millis(ITEM_PACE_MS * 2)).await;
        let held = removed(&dst, count).await;
        assert!(held < count, "the pause has to land while there's still work left");
        // The one place waiting IS the assertion: an unparked reversal would clear
        // several more items inside this window.
        // allowed-test-sleep: negative assertion over a window; a park exposes no "I am parked" signal.
        tokio::time::sleep(HELD_WINDOW).await;
        assert_eq!(
            removed(&dst, count).await,
            held,
            "a paused reversal must not remove another file"
        );

        reversal.resume();
    });

    // Resumed where it parked rather than restarted: it finishes the whole set,
    // and nothing was reversed twice.
    assert_eq!(report.final_state, RollbackState::RolledBack);
    assert_eq!(report.reversed, count as u64);
    assert!(!report.canceled);
    assert_eq!(removed(&dst, count).await, count);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stopping_inside_one_large_file_leaves_no_partial_and_loses_nothing() {
    // One file big enough to span many chunks, so the stop lands mid-stream.
    let bytes = vec![b'x'; 8 * 64 * 1024];
    let rig = Rig::new();
    let src = Arc::new(InMemoryVolume::new("Src"));
    let dst = Arc::new(InMemoryVolume::new("Dst").with_read_chunk_delay(CHUNK_DELAY));
    put(&dst, "/big.dat", &bytes).await;
    rig.register("src", Arc::clone(&src));
    rig.register("dst", Arc::clone(&dst));
    rig.seed(
        "op",
        OpKind::Move,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        vec![file_unit(0, "src", "/big.dat", "dst", "/big.dat", bytes.len() as i64)],
    );
    let reversal = Reversal::new("rollback-midfile-stop");

    let (report, ()) = tokio::join!(rig.rollback_driven_by("op", "inv-1", &reversal), async {
        // INSIDE the file: some of its bytes have moved and not all of them.
        wait_until_async(PATIENCE, "the restore to get part-way through the file", || {
            reversal
                .frames()
                .iter()
                .any(|frame| frame.bytes_done > 0 && frame.bytes_done < frame.bytes_total)
        })
        .await;
        reversal.stop();
    });

    assert!(report.canceled, "the stop must be observed inside the file");
    assert_eq!(report.reversed, 0, "the file never finished coming back");
    assert_eq!(
        report.final_state,
        RollbackState::Rollbackable,
        "nothing reversed ⇒ a clean retry"
    );

    // Whichever side holds the file holds ALL of it, and nothing half-written
    // survives on the other. This is the property `cross_volume_restore` couldn't
    // offer: its callback never answered "stop", so the file copied in full first.
    assert_eq!(
        read(&dst, "/big.dat").await,
        bytes,
        "the file is untouched where it was"
    );
    assert!(
        !exists(&src, "/big.dat").await,
        "no partial at the restore target: the staged bytes were abandoned"
    );
    let leftovers = src
        .list_directory(std::path::Path::new("/"), None)
        .await
        .expect("list the restore target's folder");
    assert!(
        leftovers.is_empty(),
        "the abandoned staging temp must be gone too, found {leftovers:?}"
    );
}

/// A runner that removes what it's told to and stops the run the moment the
/// directory sweep asks for a SECOND folder.
///
/// The sweep is the one phase with no throttle hook, no bytes to watch, and no
/// progress frames, so there's no window to press stop in from outside. Stopping
/// from INSIDE an act puts the click exactly where it has to be noticed. What's
/// under test here is the planner's polling, not the executor, so a stub is the
/// honest double.
struct StopAfterOneDir {
    dirs_removed: AtomicUsize,
}

impl RollbackRunner for StopAfterOneDir {
    fn perform<'a>(
        &'a self,
        act: InverseAct<'a>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            match act {
                InverseAct::RemoveDir { volume, path } => {
                    self.dirs_removed.fetch_add(1, Ordering::SeqCst);
                    volume.delete(path).await
                }
                InverseAct::RemoveFile { volume, path } => volume.delete(path).await,
                InverseAct::Restore { .. } => Err(VolumeError::NotSupported),
            }
        })
    }

    fn should_stop(&self) -> bool {
        self.dirs_removed.load(Ordering::SeqCst) >= 1
    }

    fn wait_while_paused(&self) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(std::future::ready(()))
    }

    fn report_progress(&self, _progress: RollbackProgress<'_>) {}
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_during_the_directory_sweep_ends_the_run() {
    // The deferred-directory phase polls the stop too. Without that, a reversal of
    // a directory-heavy operation would keep removing folders after the click, and
    // the file loop's check would never get another look-in: by then it's done.
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    mkdir(&dst, "/outer").await;
    mkdir(&dst, "/outer/inner").await;
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", Arc::clone(&dst));
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        vec![dir_unit(0, "dst", "/outer"), dir_unit(1, "dst", "/outer/inner")],
    );

    let runner = StopAfterOneDir {
        dirs_removed: AtomicUsize::new(0),
    };
    let original = rig.read_op("op");
    let report = execute_rollback(&rig.vm, &rig.writer, &original, "inv-1", Initiator::User, &runner).await;

    assert!(report.canceled, "the sweep must notice the stop");
    assert_eq!(report.reversed, 1, "the folder it was already removing still counts");
    assert!(
        exists(&dst, "/outer").await,
        "a stopped sweep removes no further folders"
    );
    assert!(!exists(&dst, "/outer/inner").await, "deepest-first: that one went");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_dispatch_names_where_the_reversal_will_act() {
    // The queue row would otherwise be nameless while it works. The name is read
    // off the NEWEST journal row at dispatch — one row, never the list.

    // A copy undo removes things and puts nothing anywhere, and its newest row is
    // the directory it created: that folder IS what the undo cleans.
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    put(&dst, "/landed/f.txt", &[b'x'; FILE_BYTES]).await;
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", Arc::clone(&dst));
    rig.seed(
        "copy-op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        vec![
            file_unit(0, "src", "/f.txt", "dst", "/landed/f.txt", FILE_BYTES as i64),
            dir_unit(1, "dst", "/landed"),
        ],
    );
    let plan = rollback_operation(&rig.vm, &rig.writer, "copy-op", |_plan| Ok(())).expect("dispatch");
    assert_eq!(plan.summary.from.as_deref(), Some("/landed"));
    assert_eq!(plan.summary.to, None, "a removal has nowhere to put anything");

    // A move undo takes items FROM where they landed and puts them back.
    let (rig, _src, _dst) = moved_across(1, Duration::ZERO).await;
    let plan = rollback_operation(&rig.vm, &rig.writer, "op", |_plan| Ok(())).expect("dispatch");
    assert_eq!(plan.summary.from.as_deref(), Some("/"));
    assert_eq!(plan.summary.to.as_deref(), Some("/"));
}

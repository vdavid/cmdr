//! Tests for the persisted in-flight temp ledger: what a launch sweeps, what it
//! defers, and what it refuses.
//!
//! Every cell records into a [`Ledger`] of its own, so its log and its tally
//! hold its own records and nothing else. A crash is the recording ledger being
//! dropped, and the next launch is a fresh ledger replaying the same data dir.
//! Volume-borne cells still use a unique volume ID: the volume registry is one
//! per process, and a shared ID would let one cell's registration wake another
//! cell's arrival listener.

use super::*;
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::manager::test_support::TestVolumeRegistration;
use crate::file_system::volume::{InMemoryVolume, Volume};
use crate::test_support::TestDir;
use cmdr_fs::staging::StagingTemp;
use cmdr_fs::testing::wait_until_async;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// How long a cell waits for the arrival listener's task to finish its deletes.
/// Generous: it's a panic-on-timeout ceiling, not a delay anything pays.
const ARRIVAL_WAIT: Duration = Duration::from_secs(5);

fn state(ledger: &Ledger) -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(50)).with_in_flight_ledger(ledger.clone()))
}

/// A state that names `volume_id` as its destination, which is what a real
/// volume copy or move builds and what tells the ledger where a staged partial
/// lives.
fn state_writing_to(ledger: &Ledger, volume_id: &str) -> Arc<WriteOperationState> {
    Arc::new(
        WriteOperationState::new(Duration::from_millis(50))
            .with_journal_volumes("some-source".to_string(), volume_id.to_string())
            .with_in_flight_ledger(ledger.clone()),
    )
}

/// The whole point of persisting: a partial recorded by a run that never
/// came back is gone at the next launch, however young it is. The
/// directory scan's one-hour gate protects against a concurrent instance;
/// a path we recorded ourselves needs no such protection, and waiting an
/// hour to clear it is the gap this closes.
#[test]
fn a_recorded_orphan_is_swept_at_startup_however_fresh_it_is() {
    let dir = TestDir::new("in_flight_temps_startup_sweep");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let previous_run = Ledger::recording_in(&data_dir);

    // A previous run: it registered a partial and then died.
    let state = state(&previous_run);
    let temp = StagingTemp::mint(&dir.join("holiday.raw"), None);
    std::fs::write(temp.path(), b"half a photo").unwrap();
    register(&state, temp.path(), Some(TempHome::LocalFs));
    let orphan = temp.path().to_path_buf();
    assert!(orphan.exists(), "the fixture partial must be on disk");
    // The process is gone: only the file in the data dir remembers it.
    drop(previous_run);

    // Joining the sweep is what keeps this honest. It runs off the startup
    // thread (a partial can live on a dead mount), and a deadline racing it
    // would fail on load rather than on a real break.
    let next_run = Ledger::for_test();
    let tally = next_run.launch_in(&data_dir).wait();

    assert!(
        !orphan.exists(),
        "the recorded orphan must be swept at startup, with no age gate"
    );
    assert_eq!(tally.swept, 1, "and the sweep must say so: {tally:?}");
    assert!(
        !next_run.live_paths().contains(&orphan),
        "and the new session must not start with the swept path in flight"
    );
    assert!(
        !read_recorded(&data_dir.join(STORE_FILENAME)).contains(&Record::Legacy(RecordedTemp::Local(orphan.clone()))),
        "the swept record must be retired on disk too, so the next launch has nothing to redo"
    );
}

/// The bug this ledger had for every non-local backend: a partial staged on
/// SMB / SFTP / WebDAV / MTP lives in the VOLUME's path space, so resolving the
/// recorded path against the local filesystem finds nothing, and the sweep did
/// nothing while reporting nothing. It has to delete through the volume.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_recorded_partial_on_a_volume_is_swept_through_that_volume() {
    let dir = TestDir::new("in_flight_temps_volume_sweep");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let previous_run = Ledger::recording_in(&data_dir);

    let volume_id = "in-flight-temps-test-nas-swept";
    let volume = Arc::new(InMemoryVolume::new("nas"));
    let orphan = PathBuf::from("/photos/holiday.raw.cmdr-tmp-4242");
    volume.create_file(&orphan, b"half a photo").await.unwrap();
    let _registration = TestVolumeRegistration::install(volume_id, Arc::clone(&volume) as Arc<dyn Volume>);

    let state = state_writing_to(&previous_run, volume_id);
    register(&state, &orphan, Some(TempHome::Volume(volume_id)));
    drop(previous_run);

    let next_run = Ledger::for_test();
    let tally = next_run.launch_in(&data_dir).wait();

    assert!(
        !volume.exists(&orphan).await,
        "the recorded partial must be gone from the volume that holds it"
    );
    assert_eq!(tally.swept, 1, "and it must be counted as swept: {tally:?}");
}

/// A record whose volume isn't reachable stays a record. Deleting it would
/// forget the only trace of a multi-gigabyte partial on a NAS the user will
/// plug back in tomorrow; ❌ and chasing the volume (mounting, dialling,
/// asking for a password) is the one thing a launch must never do.
#[test]
fn a_record_whose_volume_isnt_reachable_survives_for_the_next_launch() {
    let dir = TestDir::new("in_flight_temps_deferred");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let previous_run = Ledger::recording_in(&data_dir);

    let volume_id = "in-flight-temps-test-nas-unplugged";
    let orphan = PathBuf::from("/photos/holiday.raw.cmdr-tmp-7777");
    let state = state_writing_to(&previous_run, volume_id);
    register(&state, &orphan, Some(TempHome::Volume(volume_id)));
    drop(previous_run);

    let next_run = Ledger::for_test();
    let tally = next_run.launch_in(&data_dir).wait();

    assert_eq!(
        tally,
        SweepTally {
            deferred: 1,
            ..SweepTally::default()
        },
        "an unreachable volume's record is deferred, never swept or dropped"
    );
    assert!(
        read_recorded(&data_dir.join(STORE_FILENAME)).contains(&Record::Legacy(RecordedTemp::OnVolume(VolumeTemp {
            volume_id: volume_id.to_string(),
            path: orphan,
        }))),
        "and it has to still be on disk, or the next launch has nothing to retry"
    );
}

/// The deferral has to terminate. The moment the volume shows up in the
/// registry — the user reconnects the share, an external disk mounts — the
/// records waiting on it are cleared, without anything having to transfer into
/// that folder first.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_deferred_record_is_swept_the_moment_its_volume_arrives() {
    let dir = TestDir::new("in_flight_temps_arrival");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let previous_run = Ledger::recording_in(&data_dir);

    let volume_id = "in-flight-temps-test-nas-late";
    let orphan = PathBuf::from("/photos/holiday.raw.cmdr-tmp-8888");
    let state = state_writing_to(&previous_run, volume_id);
    register(&state, &orphan, Some(TempHome::Volume(volume_id)));
    drop(previous_run);

    // Launch with the share still away: the record is held, not acted on.
    let next_run = Ledger::for_test();
    let tally = next_run.launch_in(&data_dir).wait();
    assert_eq!(tally.deferred, 1, "the fixture must actually defer: {tally:?}");

    // The user connects it.
    let volume = Arc::new(InMemoryVolume::new("nas"));
    volume.create_file(&orphan, b"half a photo").await.unwrap();
    let _registration = TestVolumeRegistration::install(volume_id, Arc::clone(&volume) as Arc<dyn Volume>);

    wait_until_async(ARRIVAL_WAIT, "the arrived volume's partial to be swept", || {
        !next_run.live_paths().contains(&orphan)
    })
    .await;
    assert!(
        !volume.exists(&orphan).await,
        "the partial must be gone from the volume that just arrived"
    );
}

/// A volume that comes back but refuses the delete keeps its record, so a later
/// arrival (or a later launch) tries again. Losing the record on a transport
/// blip would strand the partial forever.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delete_the_volume_refuses_leaves_the_record_to_retry() {
    let dir = TestDir::new("in_flight_temps_refused_delete");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let previous_run = Ledger::recording_in(&data_dir);

    let volume_id = "in-flight-temps-test-nas-blip";
    let orphan = PathBuf::from("/photos/holiday.raw.cmdr-tmp-9999");
    let volume = Arc::new(InMemoryVolume::new("nas").with_delete_failing());
    volume.create_file(&orphan, b"half a photo").await.unwrap();
    let _registration = TestVolumeRegistration::install(volume_id, Arc::clone(&volume) as Arc<dyn Volume>);

    let state = state_writing_to(&previous_run, volume_id);
    register(&state, &orphan, Some(TempHome::Volume(volume_id)));
    drop(previous_run);

    let next_run = Ledger::for_test();
    let tally = next_run.launch_in(&data_dir).wait();

    assert_eq!(tally.swept, 0, "nothing was removed: {tally:?}");
    assert_eq!(tally.deferred, 1, "so the record has to be waiting again: {tally:?}");
    assert!(
        read_recorded(&data_dir.join(STORE_FILENAME)).contains(&Record::Legacy(RecordedTemp::OnVolume(VolumeTemp {
            volume_id: volume_id.to_string(),
            path: orphan,
        }))),
        "and it must still be on disk for the next launch"
    );
}

/// A log written by an earlier build holds bare paths with no path space. The
/// honest reading is "local": that's all the one-field format could express
/// correctly, since a volume path without its volume resolves against the local
/// filesystem. Those lines must still sweep.
#[test]
fn a_bare_path_line_from_an_older_build_still_sweeps_the_local_file() {
    let dir = TestDir::new("in_flight_temps_old_format");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let orphan = dir.join("holiday.raw.cmdr-tmp-1234");
    std::fs::write(&orphan, b"half a photo").unwrap();
    // Byte-for-byte the old format: `+` then the path as a bare JSON string.
    std::fs::write(
        data_dir.join(STORE_FILENAME),
        format!("+{}\n", serde_json::to_string(&orphan).unwrap()),
    )
    .unwrap();
    let next_run = Ledger::for_test();

    let tally = next_run.launch_in(&data_dir).wait();

    assert!(!orphan.exists(), "an old bare-path record must still be swept");
    assert_eq!(tally.swept, 1, "{tally:?}");
}

/// A local record is written in exactly the shape an older build wrote, so a
/// downgrade reads today's log without losing a line.
#[test]
fn a_local_record_stays_a_bare_path_on_disk() {
    let dir = TestDir::new("in_flight_temps_local_shape");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let ledger = Ledger::recording_in(&data_dir);

    let state = state(&ledger);
    let temp = dir.join("holiday.raw.cmdr-tmp-2222");
    register(&state, &temp, Some(TempHome::LocalFs));

    let written = std::fs::read_to_string(data_dir.join(STORE_FILENAME)).unwrap();
    assert_eq!(
        written,
        format!("+{}\n", serde_json::to_string(&temp).unwrap()),
        "a local record has to stay the bare path an older build wrote"
    );
}

/// An operation that never named its destination volume can't have its partial
/// persisted: a path recorded without its path space is one the next launch
/// could resolve against the local filesystem and act on there. The
/// operation's OWN ledger still carries it, since that sweep holds the volume
/// handle and needs no ID.
#[test]
fn a_partial_with_no_named_path_space_is_kept_in_memory_but_not_persisted() {
    let dir = TestDir::new("in_flight_temps_unnamed_home");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let ledger = Ledger::recording_in(&data_dir);

    let state = state(&ledger);
    let temp = PathBuf::from("/photos/holiday.raw.cmdr-tmp-3333");
    register(&state, &temp, None);

    assert!(
        state.in_flight_temps.lock_ignore_poison().contains(&temp),
        "the operation's own ledger still has to find it"
    );
    assert!(
        ledger.live_paths().is_empty(),
        "but nothing may be persisted about a path whose space we can't name"
    );
    assert_eq!(
        std::fs::read_to_string(data_dir.join(STORE_FILENAME)).unwrap(),
        "",
        "and nothing about it may reach the log"
    );
}

/// A temp that landed is recorded as gone, so replaying the log doesn't
/// resurrect it as an orphan. Without the `-` record every file ever copied
/// would be a sweep candidate at the next launch.
#[test]
fn a_landed_temp_is_retired_from_the_log() {
    let dir = TestDir::new("in_flight_temps_retired");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let ledger = Ledger::recording_in(&data_dir);

    let state = state(&ledger);
    let temp = dir.join("holiday.raw.cmdr-tmp-1234");
    register(&state, &temp, Some(TempHome::LocalFs));
    deregister(&state, &temp, Some(TempHome::LocalFs));

    assert!(
        !read_recorded(&data_dir.join(STORE_FILENAME)).contains(&Record::Legacy(RecordedTemp::Local(temp))),
        "a temp that came and went must replay as nothing in flight"
    );
}

/// Compaction runs on size alone, ❌ not on "nothing is in flight" — the
/// concurrent cross-volume driver holds a window open for a whole transfer,
/// so an idle-only rule would let a big copy grow megabytes. So it has to
/// survive being run while something IS in flight: the still-live partial
/// must still be there to sweep afterwards.
#[test]
fn compaction_shrinks_the_log_without_forgetting_what_is_still_in_flight() {
    let dir = TestDir::new("in_flight_temps_compaction");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let log_path = data_dir.join(STORE_FILENAME);
    let ledger = Ledger::recording_in(&data_dir);

    let state = state(&ledger);
    let long_lived = dir.join("a-big-download.iso.cmdr-tmp-0000");
    register(&state, &long_lived, Some(TempHome::LocalFs));

    // Enough churn to cross the compaction threshold several times over.
    let churned: Vec<PathBuf> = (0..400)
        .map(|i| dir.join(format!("small-file-{i:04}.txt.cmdr-tmp-{i:04}")))
        .collect();
    for churn in &churned {
        register(&state, churn, Some(TempHome::LocalFs));
        deregister(&state, churn, Some(TempHome::LocalFs));
    }

    assert!(
        std::fs::metadata(&log_path).unwrap().len() < COMPACT_ABOVE_BYTES * 2,
        "the log must stay bounded while a long transfer churns through it"
    );
    let replayed = read_recorded(&log_path);
    assert!(
        replayed.contains(&Record::Legacy(RecordedTemp::Local(long_lived))),
        "compaction must keep the partial that is still being written"
    );
    assert!(
        churned
            .iter()
            .all(|churn| !replayed.contains(&Record::Legacy(RecordedTemp::Local(churn.clone())))),
        "and must forget every partial that already landed"
    );
}

/// Compaction rewrites the log from what the ledger believes exists, and a
/// deferred orphan is part of that. If it dropped out here, a busy session
/// would quietly forget the NAS partial it was holding for the next launch.
#[test]
fn compaction_keeps_the_records_waiting_for_a_volume() {
    let dir = TestDir::new("in_flight_temps_compaction_pending");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let log_path = data_dir.join(STORE_FILENAME);
    let previous_run = Ledger::recording_in(&data_dir);

    let volume_id = "in-flight-temps-test-nas-compacted";
    let waiting = PathBuf::from("/photos/holiday.raw.cmdr-tmp-5555");
    register(
        &state_writing_to(&previous_run, volume_id),
        &waiting,
        Some(TempHome::Volume(volume_id)),
    );
    drop(previous_run);
    let next_run = Ledger::for_test();
    next_run.launch_in(&data_dir).wait();

    // A local copy in the new session churns the log past compaction.
    let state = state(&next_run);
    for i in 0..400 {
        let churn = dir.join(format!("small-file-{i:04}.txt.cmdr-tmp-{i:04}"));
        register(&state, &churn, Some(TempHome::LocalFs));
        deregister(&state, &churn, Some(TempHome::LocalFs));
    }

    assert!(
        read_recorded(&log_path).contains(&Record::Legacy(RecordedTemp::OnVolume(VolumeTemp {
            volume_id: volume_id.to_string(),
            path: waiting,
        }))),
        "a compaction mid-session must not forget the orphan waiting for its volume"
    );
}

/// The log is a stream of appends, so the process can die mid-`write`. The
/// replay has to drop the torn tail and keep everything before it — the
/// records before the tear are the ones naming real orphans.
#[test]
fn a_torn_last_line_doesnt_cost_the_records_before_it() {
    let dir = TestDir::new("in_flight_temps_torn");
    let good = dir.join("a.raw.cmdr-tmp-1111");
    let log = dir.join(STORE_FILENAME);
    let mut contents = format!("+{}\n", serde_json::to_string(&good).unwrap());
    contents.push_str("+\"/half-a-pa"); // the process died here
    std::fs::write(&log, contents).unwrap();

    assert_eq!(read_recorded(&log), vec![Record::Legacy(RecordedTemp::Local(good))]);
}

/// A temp that landed under its real name before the crash leaves a
/// recorded path pointing at nothing. That's the common case, and it must be
/// COUNTED rather than swallowed: a sweep that can't tell "already gone" from
/// "I couldn't reach it" is a backstop nobody can check.
#[test]
fn a_recorded_path_that_is_already_gone_is_counted_not_swallowed() {
    let dir = TestDir::new("in_flight_temps_already_gone");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let gone = dir.join("gone.txt.cmdr-tmp-abc");
    std::fs::write(
        data_dir.join(STORE_FILENAME),
        format!("+{}\n", serde_json::to_string(&gone).unwrap()),
    )
    .unwrap();
    let next_run = Ledger::for_test();

    // Joining also keeps the sweep from outliving `dir`, which would leave
    // it walking a directory `TestDir` is deleting.
    let tally = next_run.launch_in(&data_dir).wait();

    assert_eq!(
        tally,
        SweepTally {
            already_gone: 1,
            ..SweepTally::default()
        },
        "the record has to land in exactly one counter: {tally:?}"
    );
    assert!(
        !next_run.live_paths().contains(&gone),
        "a record whose file is already gone must not come back as in flight"
    );
}

/// The same, on a volume: a partial that landed before the crash reports as
/// already gone rather than as a delete nobody can account for.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_path_that_is_already_gone_is_counted_not_swallowed() {
    let dir = TestDir::new("in_flight_temps_volume_already_gone");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let previous_run = Ledger::recording_in(&data_dir);

    let volume_id = "in-flight-temps-test-nas-landed";
    let volume = Arc::new(InMemoryVolume::new("nas"));
    let landed = PathBuf::from("/photos/holiday.raw.cmdr-tmp-6666");
    let _registration = TestVolumeRegistration::install(volume_id, Arc::clone(&volume) as Arc<dyn Volume>);

    // Recorded, then it landed under its real name: the file was never there
    // by the time the next launch looked.
    register(
        &state_writing_to(&previous_run, volume_id),
        &landed,
        Some(TempHome::Volume(volume_id)),
    );
    drop(previous_run);

    let next_run = Ledger::for_test();
    let tally = next_run.launch_in(&data_dir).wait();

    assert_eq!(
        tally,
        SweepTally {
            already_gone: 1,
            ..SweepTally::default()
        },
        "{tally:?}"
    );
}

/// The sweep deletes files. It follows the ledger, so it checks that what
/// the ledger names really is one of our scratch files before removing it —
/// a corrupted or hand-edited store must not become a delete-anything
/// primitive.
#[test]
fn the_sweep_refuses_a_recorded_path_that_isnt_one_of_our_scratch_files() {
    let dir = TestDir::new("in_flight_temps_not_ours");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let precious = dir.join("taxes.pdf");
    std::fs::write(&precious, b"the user's own file").unwrap();
    // A real temp recorded alongside it, as the positive control: with the
    // sweep joined below, this one being gone proves the sweep ran and does
    // delete, so `taxes.pdf` surviving is the marker check saving it rather
    // than the sweep never getting that far.
    let real_temp = dir.join("holiday.raw.cmdr-tmp-9999");
    std::fs::write(&real_temp, b"half a photo").unwrap();
    std::fs::write(
        data_dir.join(STORE_FILENAME),
        format!(
            "+{}\n+{}\n",
            serde_json::to_string(&precious).unwrap(),
            serde_json::to_string(&real_temp).unwrap()
        ),
    )
    .unwrap();
    let next_run = Ledger::for_test();

    let tally = next_run.launch_in(&data_dir).wait();

    assert!(
        !real_temp.exists(),
        "the sweep must remove the recorded partial carrying our scratch marker"
    );
    assert!(
        precious.exists(),
        "the sweep must only ever remove files carrying our scratch marker"
    );
    // Both counted, and both retired, says the sweep VISITED each one and
    // decided, which is what separates "it refused `taxes.pdf`" from "it never
    // got that far".
    assert_eq!(
        tally,
        SweepTally {
            swept: 1,
            left_alone: 1,
            ..SweepTally::default()
        },
        "one removed, one refused"
    );
    assert!(
        read_recorded(&data_dir.join(STORE_FILENAME)).is_empty(),
        "and neither is left to be retried forever"
    );
}

/// The same refusal on the volume side, where the sweep now has a `delete`
/// that really would remove whatever it's pointed at.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_sweep_refuses_a_volume_path_that_isnt_one_of_our_scratch_files() {
    let dir = TestDir::new("in_flight_temps_volume_not_ours");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let previous_run = Ledger::recording_in(&data_dir);

    let volume_id = "in-flight-temps-test-nas-tampered";
    let volume = Arc::new(InMemoryVolume::new("nas"));
    let precious = PathBuf::from("/papers/taxes.pdf");
    volume.create_file(&precious, b"the user's own file").await.unwrap();
    let _registration = TestVolumeRegistration::install(volume_id, Arc::clone(&volume) as Arc<dyn Volume>);

    register(
        &state_writing_to(&previous_run, volume_id),
        &precious,
        Some(TempHome::Volume(volume_id)),
    );
    drop(previous_run);

    let next_run = Ledger::for_test();
    let tally = next_run.launch_in(&data_dir).wait();

    assert!(
        volume.exists(&precious).await,
        "the sweep must only ever remove files carrying our scratch marker"
    );
    // Counted and retired says the sweep reached it and refused, rather than
    // never arriving.
    assert_eq!(
        tally,
        SweepTally {
            left_alone: 1,
            ..SweepTally::default()
        },
        "the refusal is counted"
    );
    assert!(
        read_recorded(&data_dir.join(STORE_FILENAME)).is_empty(),
        "and the refused record is settled, not left to be retried forever"
    );
}

/// Registering and deregistering keep both ledgers in step, so nothing
/// sweeps a file that landed.
#[test]
fn deregistering_clears_both_ledgers() {
    let dir = TestDir::new("in_flight_temps_both_ledgers");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let ledger = Ledger::recording_in(&data_dir);

    let state = state(&ledger);
    let temp = dir.join("notes.txt.cmdr-tmp-1234");
    register(&state, &temp, Some(TempHome::LocalFs));
    assert_eq!(state.in_flight_temps.lock_ignore_poison().len(), 1);
    assert_eq!(ledger.live_paths(), vec![temp.clone()]);

    deregister(&state, &temp, Some(TempHome::LocalFs));
    assert!(state.in_flight_temps.lock_ignore_poison().is_empty());
    assert!(ledger.live_paths().is_empty());
    assert!(
        !read_recorded(&data_dir.join(STORE_FILENAME)).contains(&Record::Legacy(RecordedTemp::Local(temp))),
        "the log must replay as nothing in flight"
    );
}

/// A state whose destination side is a removable drive rooted at `root`, the
/// way every transfer the dialog starts onto a USB stick is built.
fn state_writing_to_drive(ledger: &Ledger, volume_id: &str, root: &Path) -> Arc<WriteOperationState> {
    use crate::file_system::write_operations::transfer_sides::{TransferSide, TransferSides};
    Arc::new(
        WriteOperationState::new(Duration::from_millis(50))
            .with_sides(Some(TransferSides::new(
                TransferSide::new("root".to_string(), "Macintosh HD".to_string(), PathBuf::from("/")),
                TransferSide::new(volume_id.to_string(), "Fältkamera".to_string(), root.to_path_buf()),
            )))
            .with_in_flight_ledger(ledger.clone()),
    )
}

/// ❗ A build without the kinded records must FORGET them, never act on them.
/// Its reader skips an op byte it doesn't know, and its launch truncates the
/// log, so a rollback loses the records rather than putting an aside through the
/// one rule that removes things on sight.
///
/// The reader here is a copy of the old one, kept deliberately literal: the
/// point is what a DIFFERENT binary does with today's file.
#[test]
fn a_build_without_kinded_records_skips_them_rather_than_acting_on_them() {
    let dir = TestDir::new("in_flight_temps_old_reader");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let ledger = Ledger::recording_in(&data_dir);

    let state = state(&ledger);
    let aside = dir.join("notes.txt.cmdr-temp-1234");
    let survivor = dir.join("holiday.raw.cmdr-tmp-5678");
    track(
        &state,
        ItemKind::FileAside {
            destination: dir.join("notes.txt"),
            expected_size: 12,
        },
        &aside,
    );
    register(&state, &survivor, Some(TempHome::LocalFs));

    // What a build that only knows `+` and `-` would replay.
    let contents = std::fs::read_to_string(data_dir.join(STORE_FILENAME)).unwrap();
    let mut old_reader_saw: Vec<PathBuf> = Vec::new();
    for line in contents.lines() {
        let Some((op, encoded)) = line.split_at_checked(1) else {
            continue;
        };
        if op != "+" {
            continue;
        }
        if let Ok(path) = serde_json::from_str::<PathBuf>(encoded) {
            old_reader_saw.push(path);
        }
    }

    assert_eq!(
        old_reader_saw,
        vec![survivor],
        "an older build replays the plain temp and nothing else; the aside must be invisible to it"
    );
}

/// The bug item 2 fixes: a partial on a drive that's unplugged at the next
/// launch used to resolve against a mount point that isn't there, read as
/// already gone, and take the only trace of it with it. Homed to the VOLUME, it
/// waits for the drive instead.
#[test]
fn a_leftover_on_a_removable_drive_waits_for_that_drive_rather_than_being_forgotten() {
    let dir = TestDir::new("in_flight_temps_drive_homed");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let previous_run = Ledger::recording_in(&data_dir);

    let volume_id = "in-flight-temps-test-stick-away";
    let drive_root = dir.join("Volumes").join("Faltkamera");
    let state = state_writing_to_drive(&previous_run, volume_id, &drive_root);
    let temp = drive_root.join("footage.mov.cmdr-tmp-2468");
    track(&state, ItemKind::Temp, &temp);
    drop(previous_run);

    // The drive isn't plugged in, so nothing is registered for it.
    let next_run = Ledger::for_test();
    let tally = next_run.launch_in(&data_dir).wait();

    assert_eq!(
        tally,
        SweepTally {
            deferred: 1,
            ..SweepTally::default()
        },
        "a drive that isn't here decides nothing, rather than resolving against an empty mount point"
    );
    let replayed = read_recorded(&data_dir.join(STORE_FILENAME));
    let [kept] = replayed.as_slice() else {
        panic!("the record survives the launch, alone: {replayed:?}");
    };
    assert_eq!(kept.path(), Path::new("footage.mov.cmdr-tmp-2468"));
    assert_eq!(
        kept.volume_id(),
        Some(volume_id),
        "and it names the drive it waits on, so plugging it back in settles it"
    );
    // Finding it by that relative path IS the claim: stored whole, it would read
    // as an absolute path under a mount point that may never come back.
}

/// A Mac-internal path has nothing to wait for, so it stays local-homed and the
/// launch sweep answers for it before the volume registry even exists.
#[test]
fn a_leftover_on_the_mac_stays_local_homed() {
    let dir = TestDir::new("in_flight_temps_mac_homed");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let ledger = Ledger::recording_in(&data_dir);

    // The destination side is the boot volume, which is what a Mac-to-Mac copy
    // and a drive-to-Mac move both carry.
    let state = state_writing_to_drive(&ledger, "root", Path::new("/"));
    let temp = dir.join("notes.txt.cmdr-tmp-1357");
    track(&state, ItemKind::Temp, &temp);

    let replayed = read_recorded(&data_dir.join(STORE_FILENAME));
    let [recorded] = replayed.as_slice() else {
        panic!("the temp is recorded, alone: {replayed:?}");
    };
    assert_eq!(recorded.path(), temp, "under its whole path");
    assert_eq!(recorded.volume_id(), None, "nothing to wait for");
}

/// The registry's arrival announcement is what makes a deferred record
/// terminate, so it has to actually fire for every way a volume gets in.
#[test]
fn every_registration_path_announces_the_volume() {
    let heard = Arc::new(Mutex::new(Vec::<String>::new()));
    let recorder = Arc::clone(&heard);
    get_volume_manager().on_volume_arrival(move |id| {
        if id.starts_with("in-flight-temps-arrival-probe") {
            recorder.lock_ignore_poison().push(id.to_string());
        }
    });

    let manager = get_volume_manager();
    let registered = manager.register_if_absent(
        "in-flight-temps-arrival-probe-absent",
        Arc::new(InMemoryVolume::new("probe")) as Arc<dyn Volume>,
    );
    assert!(registered, "the fixture id must be free");
    manager.register(
        "in-flight-temps-arrival-probe-register",
        Arc::new(InMemoryVolume::new("probe")) as Arc<dyn Volume>,
    );
    manager.unregister("in-flight-temps-arrival-probe-absent");
    manager.unregister("in-flight-temps-arrival-probe-register");

    let heard = heard.lock_ignore_poison().clone();
    assert!(
        heard.contains(&"in-flight-temps-arrival-probe-absent".to_string())
            && heard.contains(&"in-flight-temps-arrival-probe-register".to_string()),
        "both registration paths must announce; heard {heard:?}"
    );
}

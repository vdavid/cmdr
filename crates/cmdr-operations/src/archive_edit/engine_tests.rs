//! `engine.rs`'s cells: the paced sleep, the status cache a compress's phases
//! reach, and what an edit's terminal event names of what it left behind.

use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::testing::TestOperations;

#[test]
fn paced_sleep_gives_way_to_a_cancel_within_a_slice() {
    // The cancel lands after the first check, so a 10 s pace must return at once.
    let checks = AtomicUsize::new(0);
    let started = Instant::now();
    sleep_unless_cancelled(Duration::from_secs(10), || checks.fetch_add(1, Ordering::Relaxed) >= 1);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "slept {:?} past a cancel",
        started.elapsed()
    );
}

#[test]
fn paced_sleep_runs_its_full_length_without_a_cancel() {
    let started = Instant::now();
    sleep_unless_cancelled(Duration::from_millis(30), || false);
    assert!(started.elapsed() >= Duration::from_millis(30));
}

/// What a query API (the MCP `cmdr://state` resource) reads for this op.
fn cached(engine: &TestOperations, id: &str) -> (WriteOperationPhase, usize, usize, u64, u64) {
    let status = engine
        .get_operation_status(&cmdr_operation_log::types::OperationId::from(id))
        .expect("the op is registered");
    (
        status.phase,
        status.files_done,
        status.files_total,
        status.bytes_done,
        status.bytes_total,
    )
}

#[test]
fn every_emitted_phase_of_a_compress_reaches_the_status_cache() {
    let engine = TestOperations::new();
    let id = "compress-status-mirror";
    engine.status().register(id, WriteOperationType::Compress, vec![]);
    let hooks = MutatorHooks::new(
        Arc::new(engine.new_operation_state(Duration::ZERO)),
        Arc::new(CollectorEventSink::new()) as Arc<dyn OperationEventSink>,
        id.to_string(),
        WriteOperationType::Compress,
        Duration::ZERO,
    );

    hooks.emit_scan_progress(
        super::super::fresh_plan::PlanProgress {
            files: 2,
            dirs: 1,
            bytes: 100,
            current_dir: Path::new("/src"),
        },
        None,
        true,
    );
    assert_eq!(cached(&engine, id), (WriteOperationPhase::Scanning, 2, 0, 100, 0));

    // Pre-fix the cache stayed at `Scanning` from here to the terminal event.
    let tick = |entries_done, bytes_done| MutationProgress {
        entries_done,
        entries_total: 3,
        entries_changed: 3,
        bytes_done,
        bytes_total: 100,
    };
    MutationHooks::on_progress(&hooks, tick(1, 40));
    assert_eq!(cached(&engine, id), (WriteOperationPhase::Compressing, 1, 3, 40, 100));

    MutationHooks::on_progress(&hooks, tick(3, 100));
    assert_eq!(
        cached(&engine, id),
        (WriteOperationPhase::FinishingCompression, 0, 0, 0, 0)
    );

    hooks.emit_remote_progress(RemoteCommitProgress::Transferring {
        bytes_done: 0,
        bytes_total: 60,
    });
    assert_eq!(cached(&engine, id), (WriteOperationPhase::Transferring, 0, 0, 0, 60));

    hooks.emit_remote_progress(RemoteCommitProgress::Finishing);
    assert_eq!(
        cached(&engine, id),
        (WriteOperationPhase::FinishingTransfer, 0, 0, 0, 0)
    );
}

/// A Cancel that ended a remote edit's swap (or a compress's publish) after
/// the way was cleared kept the new archive recorded for the drive's next
/// arrival. `write-cancelled` names it, as a copy's does, so the person
/// reads where their archive is rather than nothing at all.
#[test]
fn a_cancelled_edit_names_the_landing_its_stop_kept() {
    let engine = TestOperations::new();
    let nas: Arc<dyn Volume> = Arc::new(cmdr_fs::volume::InMemoryVolume::new("Naspolya"));
    engine.volumes().register("archive-terminal-nas", nas);
    let state = Arc::new(engine.new_operation_state(Duration::ZERO));
    state
        .kept_landings
        .lock_ignore_poison()
        .push(PathBuf::from("/share/bundle.zip"));
    let events = CollectorEventSink::new();

    emit_archive_terminal(
        &events,
        "archive-terminal-cancel",
        WriteOperationType::ArchiveEdit,
        Err(EditError::Cancelled),
        0,
        EditLeftovers::of(&state, "archive-terminal-nas", None),
        &MutationProgress::default(),
    );

    let cancelled = events.cancelled.lock_ignore_poison();
    let [cancelled] = cancelled.as_slice() else {
        panic!("one `write-cancelled`, got {cancelled:?}");
    };
    assert_eq!(
        cancelled.rollback.staged_leftovers,
        vec![StagedLeftovers {
            fate: StagedLeftoverFate::LandsWhenBack,
            count: 1,
            example_name: "bundle.zip".to_owned(),
            volume_name: "Naspolya".to_owned(),
        }],
    );
    assert!(
        state.kept_landings.lock_ignore_poison().is_empty(),
        "named once, by the one terminal event"
    );
}

/// A failed edit names the new archive its stop kept too, beside the cause,
/// so nobody finds a ` (recovered)` archive with no word about it.
#[test]
fn a_failed_edit_names_the_new_archive_its_stop_kept_beside_its_cause() {
    let engine = TestOperations::new();
    let nas: Arc<dyn Volume> = Arc::new(cmdr_fs::volume::InMemoryVolume::new("Naspolya"));
    engine.volumes().register("archive-terminal-nas", nas);
    let state = Arc::new(engine.new_operation_state(Duration::ZERO));
    state
        .rescued_landings
        .lock_ignore_poison()
        .push(PathBuf::from("/share/bundle (recovered).zip"));
    let events = CollectorEventSink::new();
    let cause = WriteOperationError::IoError {
        path: "/share/other.zip".to_owned(),
        message: "the share's own words".to_owned(),
    };

    emit_archive_terminal(
        &events,
        "archive-terminal-error",
        WriteOperationType::ArchiveEdit,
        Err(EditError::Op(cause)),
        0,
        EditLeftovers::of(&state, "archive-terminal-nas", None),
        &MutationProgress::default(),
    );

    let errors = events.errors.lock_ignore_poison();
    let [failed] = errors.as_slice() else {
        panic!("one `write-error`, got {errors:?}");
    };
    let WriteOperationError::NewFilesKept { cause: reported, kept } = &failed.error else {
        panic!("the failure names the kept archive, got {:?}", failed.error);
    };
    assert!(
        matches!(&**reported, WriteOperationError::IoError { path, .. } if path == "/share/other.zip"),
        "the cause stays what failed, got {reported:?}"
    );
    assert_eq!(
        kept,
        &vec![StagedLeftovers {
            fate: StagedLeftoverFate::Rescued,
            count: 1,
            example_name: "bundle (recovered).zip".to_owned(),
            volume_name: "Naspolya".to_owned(),
        }],
    );
}

/// ❗ A remote edit or a compress that stops while its share is out can't
/// delete its temp (`StagedWrite::abandon` leaves it listed). Its terminal
/// hands the temp to the background cleanup on the parent's drive, as a
/// copy's stop does, and what that can't remove waits for the drive's next
/// arrival, ❌ never the next launch (#574). `write-cancelled` says so.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_temp_a_stopped_edit_left_on_a_share_that_is_out_waits_for_the_drives_next_arrival() {
    use crate::in_flight_temps::Ledger;
    use crate::outage_slot::{CleanupPolicy, OutagePolicy};
    use crate::transfer::outage_volume_test_support::OutageVolume;
    use cmdr_fs::volume::InMemoryVolume;

    const NAS_ID: &str = "archive-terminal-nas-out";
    const CADENCE: &[Duration] = &[Duration::from_millis(10)];
    let engine = TestOperations::new();
    let ledger = Ledger::for_test();
    let inner = Arc::new(InMemoryVolume::new("Naspolya"));
    let nas = OutageVolume::wrap(Arc::clone(&inner), NAS_ID);
    engine.volumes().register(NAS_ID, Arc::clone(&nas) as Arc<dyn Volume>);
    let temp = PathBuf::from("/share/bundle.zip.cmdr-tmp-77aa");
    inner.create_file(&temp, b"half an archive").await.unwrap();
    let state = Arc::new(
        engine
            .new_operation_state(Duration::ZERO)
            .with_in_flight_ledger(ledger.clone())
            .with_outage_policy(OutagePolicy {
                budget: Duration::from_secs(2),
                cadence: CADENCE,
                poll_every: Duration::from_millis(5),
                attempt_timeout: Duration::from_millis(100),
            })
            .with_cleanup_policy(CleanupPolicy {
                inline_budget: Duration::from_secs(5),
                wait: Duration::from_millis(50),
                retry_after: Duration::from_millis(20),
            }),
    );
    in_flight_temps::track_temp(&state, Some(RecordHome::volume_space(NAS_ID)), &temp);
    nas.go_away();
    let events = CollectorEventSink::new();

    emit_archive_terminal(
        &events,
        "archive-terminal-temp-left",
        WriteOperationType::ArchiveEdit,
        Err(EditError::Cancelled),
        0,
        EditLeftovers::of(&state, NAS_ID, None),
        &MutationProgress::default(),
    );
    tokio::time::timeout(Duration::from_secs(5), engine.leftover_cleanups_settled())
        .await
        .expect("the background pass settles");

    assert_eq!(
        ledger.pending_paths(),
        vec![temp.clone()],
        "the temp waits for the drive's next arrival"
    );
    assert!(
        state.in_flight_temps.lock_ignore_poison().is_empty(),
        "handed off once, by the one terminal"
    );
    let cancelled = events.cancelled.lock_ignore_poison();
    let [cancelled] = cancelled.as_slice() else {
        panic!("one `write-cancelled`, got {cancelled:?}");
    };
    assert_eq!(
        cancelled.rollback.staged_leftovers,
        vec![StagedLeftovers {
            fate: StagedLeftoverFate::RemovedWhenBack,
            count: 1,
            example_name: "bundle.zip.cmdr-tmp-77aa".to_owned(),
            volume_name: "Naspolya".to_owned(),
        }],
    );
}

#[test]
fn remote_transfer_ticks_keep_the_configured_cadence_but_phase_boundaries_emit() {
    let engine = TestOperations::new();
    let events = Arc::new(CollectorEventSink::new());
    let hooks = MutatorHooks::new(
        Arc::new(engine.new_operation_state(Duration::from_secs(60))),
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        "compress-cadence".to_string(),
        WriteOperationType::Compress,
        Duration::from_secs(60),
    );

    hooks.emit_remote_progress(RemoteCommitProgress::Transferring {
        bytes_done: 0,
        bytes_total: 100,
    });
    hooks.emit_remote_progress(RemoteCommitProgress::Transferring {
        bytes_done: 10,
        bytes_total: 100,
    });
    hooks.emit_remote_progress(RemoteCommitProgress::Transferring {
        bytes_done: 20,
        bytes_total: 100,
    });
    hooks.emit_remote_progress(RemoteCommitProgress::Finishing);

    let progress = events.progress.lock_ignore_poison();
    assert_eq!(progress.len(), 2, "intermediate upload chunks must be throttled");
    assert_eq!(progress[0].phase, WriteOperationPhase::Transferring);
    assert_eq!(progress[0].bytes_done, 0);
    assert_eq!(progress[1].phase, WriteOperationPhase::FinishingTransfer);
}

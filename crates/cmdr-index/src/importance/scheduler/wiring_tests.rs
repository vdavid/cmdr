//! Wiring tests: what actually fires for a volume the scheduler learns about
//! through the REGISTRATION bus rather than the startup sweep.
//!
//! The sweep-time decision is covered by `multi_volume_tests`'
//! `should_enqueue_full_pass_gates_on_kind_and_store_state`, which drives the
//! decision function directly. These drive `wire_volume` itself, because that's the
//! half a decision function can't vouch for: a volume that becomes ready AFTER
//! `start()` never appears in the sweep, so a probe only the sweep runs is dead code
//! on a real launch.

use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::testing::wait_until;

use super::test_support::*;
use super::wiring::wire_volume;
use super::*;
use crate::IndexVolumeKind;
use crate::importance::store::{RECOMPUTE_GENERATION_KEY, SCORING_POLICY_KEY, open_read_connection, read_meta_value};
use crate::indexing::lifecycle::lifecycle_bus;

/// How long a spawned full pass gets to land before the test calls it a failure.
/// Generous on purpose: the pass runs on a background blocking task, and a loaded
/// CI machine is slow.
const PASS_LANDS_WITHIN: Duration = Duration::from_secs(15);

/// How long the already-scored test lets a would-be pass land before concluding
/// nothing was kicked. The kick test's pass lands in well under a tenth of this, so
/// a regression that re-armed an unconditional kick would be caught, not raced.
const NO_PASS_SETTLE: Duration = Duration::from_secs(2);

/// A volume id no other test touches, so the process-global lifecycle buses and
/// read-pool registry stay in the shape this test needs. Its scan bus retains
/// `Pending` (never a `Completed` some other test published for `root`), which IS
/// the Fresh-at-launch shape.
const LATE_VOLUME_ID: &str = "wiring-late-registration";

/// The same, for the already-scored case. A separate id keeps the two tests'
/// process-global buses and read pools out of each other's way under `cargo test`'s
/// parallelism.
const SCORED_VOLUME_ID: &str = "wiring-already-scored";

/// One meta value from the volume's store, or `None` when the file or the key is
/// absent. Read-only, so polling it never contends with the writer thread that's
/// mid-pass.
fn meta_value(data_dir: &std::path::Path, volume_id: &str, key: &str) -> Option<String> {
    let db = importance_db_path(data_dir, volume_id);
    if !db.exists() {
        return None;
    }
    open_read_connection(&db)
        .ok()
        .and_then(|conn| read_meta_value(&conn, key).ok().flatten())
}

/// Whether the volume's store carries this build's scoring-policy stamp — the
/// on-disk proof that a FULL pass ran, since `apply_full_pass` is the only writer of
/// that key.
fn store_is_stamped(data_dir: &std::path::Path, volume_id: &str) -> bool {
    meta_value(data_dir, volume_id, SCORING_POLICY_KEY).as_deref()
        == Some(crate::importance::classify::scoring_policy_fingerprint().as_str())
}

/// The registration path owes a volume its full-pass probe, exactly as the startup
/// sweep does.
///
/// The root index starts on a spawned task and `ImportanceScheduler::start()` runs
/// synchronously right after it, so on a real launch the sweep sees an EMPTY registry
/// and root arrives later on the registration bus. A probe only the sweep ran would be
/// unreachable in production: neither the no-generation initial pass nor the
/// scoring-policy re-arm would ever fire, and the volume would coast forever on
/// incremental rescores under superseded classification rules.
///
/// Nothing else can score the volume here: its lifecycle bus stays `Pending` (no
/// `ScanCompleted` is ever published), no dir-changed batch is published, and the
/// periodic refresh is an hour out. So the stamp landing proves the probe ran from
/// `wire_volume`.
#[test]
fn wire_volume_probes_for_a_full_pass_for_a_volume_that_registers_after_start() {
    let dir = tempfile::tempdir().expect("temp dir");
    install_index_for(dir.path(), LATE_VOLUME_ID);

    let scheduler = Arc::new(ImportanceScheduler::new(dir.path().to_path_buf()));
    assert!(
        !store_is_stamped(dir.path(), LATE_VOLUME_ID),
        "the store starts unscored (no full pass has run)"
    );

    // Exactly what the registration-bus handler does for a volume that registers
    // after the sweep already ran.
    wire_volume(
        Arc::clone(&scheduler),
        LATE_VOLUME_ID.to_string(),
        IndexVolumeKind::Local,
        CancellationToken::new(),
    );

    wait_until(
        PASS_LANDS_WITHIN,
        "the registration path to run the full-pass probe and stamp the scoring policy",
        || store_is_stamped(dir.path(), LATE_VOLUME_ID),
    );

    crate::indexing::read::enrichment::uninstall_read_pool(LATE_VOLUME_ID);
}

/// And it stays a PROBE, never an unconditional kick: wiring a volume whose store is
/// already scored under this build's policy starts no pass.
///
/// The gate is the whole reason importance doesn't copy media's cheap
/// kick-everything-on-launch: a full pass costs ~5.8 s CPU and a ~166 MB transient
/// allocation on the boot volume, so rescoring every volume on every launch is the
/// treadmill `docs/notes/performance/importance-treadmill-2026-08-04.md` exists to document. Now
/// that the probe runs from `wire_volume` (so it fires on every registration, not
/// only the sweep), that cost sits behind this one check.
#[test]
fn wire_volume_does_not_kick_a_pass_for_an_already_scored_volume() {
    let dir = tempfile::tempdir().expect("temp dir");
    install_index_for(dir.path(), SCORED_VOLUME_ID);

    // Seed a completed full pass: generation 1, stamped with this build's policy.
    let writer = ImportanceWriter::spawn(&importance_db_path(dir.path(), SCORED_VOLUME_ID)).expect("writer");
    writer
        .write_weights(
            1,
            vec![WeightRow {
                path: "/keep".to_string(),
                score: 0.9,
                signals_json: "{}".to_string(),
            }],
        )
        .expect("write weights");
    writer.flush_blocking().expect("flush");
    writer.shutdown();
    assert_eq!(
        meta_value(dir.path(), SCORED_VOLUME_ID, RECOMPUTE_GENERATION_KEY).as_deref(),
        Some("1"),
        "the seeded store is already scored at generation 1"
    );

    let scheduler = Arc::new(ImportanceScheduler::new(dir.path().to_path_buf()));
    wire_volume(
        Arc::clone(&scheduler),
        SCORED_VOLUME_ID.to_string(),
        IndexVolumeKind::Local,
        CancellationToken::new(),
    );

    // allowed-test-sleep: the settle IS the subject — the assertion is that NOTHING happens in it.
    std::thread::sleep(NO_PASS_SETTLE);
    assert_eq!(
        meta_value(dir.path(), SCORED_VOLUME_ID, RECOMPUTE_GENERATION_KEY).as_deref(),
        Some("1"),
        "an already-scored volume is left alone: no pass bumped the generation"
    );

    crate::indexing::read::enrichment::uninstall_read_pool(SCORED_VOLUME_ID);
}

/// Forgetting a share deletes its importance database out from under the writer
/// thread the scheduler keeps per volume, so the scheduler has to let go FIRST.
///
/// Two things go wrong when it doesn't, and both are pinned here. The writer's
/// connection keeps the unlinked file's blocks allocated (tens of MB on a big
/// share) for the rest of the session. And the writer stays in the registry, so
/// when the share is indexed again every write goes into that unlinked file: the
/// new database on disk never receives a row.
#[test]
fn a_forgotten_volumes_writer_lets_go_of_its_database() {
    use crate::volume_files::{self, Removal, StoreDirs};
    const VOLUME_ID: &str = "smb-wiring-forgotten";

    let dir = tempfile::tempdir().expect("temp dir");
    let scheduler = Arc::new(ImportanceScheduler::new(dir.path().to_path_buf()));
    wiring::hold_the_importance_store(&scheduler);
    let importance_db = importance_db_path(dir.path(), VOLUME_ID);
    let visits = |db: &std::path::Path| -> i64 {
        open_read_connection(db)
            .and_then(|conn| Ok(conn.query_row("SELECT COUNT(*) FROM visits", [], |row| row.get(0))?))
            .unwrap_or(-1)
    };

    let first = scheduler.writer_for(VOLUME_ID).expect("writer");
    first.record_visit("/share/photos", 1).expect("visit");
    first.flush_blocking().expect("flush");
    let files = cmdr_fs::sqlite_util::database_files(&importance_db);
    assert!(
        files.iter().all(|file| file.exists()),
        "test setup: a live writer keeps the database, its WAL, and its SHM on disk"
    );

    volume_files::remove(&StoreDirs::single(dir.path()), VOLUME_ID, Removal::Forgotten).expect("forget");

    for file in &files {
        assert!(!file.exists(), "{} must be gone", file.display());
    }
    assert!(
        first.record_visit("/share/photos", 2).is_err(),
        "the old writer is shut down, so nothing can write into the unlinked file"
    );

    // The share is indexed again: a new writer, on a new database.
    let second = scheduler.writer_for(VOLUME_ID).expect("a fresh writer");
    second.record_visit("/share/music", 3).expect("visit");
    second.flush_blocking().expect("flush");
    assert_eq!(
        visits(&importance_db),
        1,
        "the new database holds the new visit and nothing of the forgotten one"
    );
}

/// A volume's listeners last exactly as long as the life of the volume they were
/// wired for.
///
/// Every start of a volume publishes a registration, so every start wires the volume
/// again: a share that reconnects, a drive turned off and back on, the master switch
/// toggled. Pre-fix the listeners outlived their volume, so they piled up one set per
/// start, and each set carries its own hourly full-refresh timer: a share that had
/// reconnected ten times was rescored in full ten times an hour. The buses' receiver
/// counts are the observable, since a listener holds its receiver for as long as it
/// runs.
#[test]
fn a_volumes_listeners_end_when_the_volume_stops() {
    const VOLUME_ID: &str = "wiring-listeners-end";
    /// The scan-completion, home-coverage, and dir-changed subscriptions.
    const LISTENING: usize = 3;

    let dir = tempfile::tempdir().expect("temp dir");
    let scheduler = Arc::new(ImportanceScheduler::new(dir.path().to_path_buf()));
    let volume_root = CancellationToken::new();

    wire_volume(
        Arc::clone(&scheduler),
        VOLUME_ID.to_string(),
        IndexVolumeKind::Local,
        volume_root.child_token(),
    );
    assert_eq!(
        lifecycle_bus::subscriber_count_for_test(VOLUME_ID),
        LISTENING,
        "a wired volume is listened to"
    );

    // The volume stops: its root signal fires, and the wiring's child with it.
    volume_root.cancel();
    wait_until(
        Duration::from_secs(5),
        "the stopped volume's listeners to let go of its buses",
        || lifecycle_bus::subscriber_count_for_test(VOLUME_ID) == 0,
    );

    // It starts again, which wires it again: one set of listeners, not two.
    wire_volume(
        Arc::clone(&scheduler),
        VOLUME_ID.to_string(),
        IndexVolumeKind::Local,
        CancellationToken::new(),
    );
    assert_eq!(lifecycle_bus::subscriber_count_for_test(VOLUME_ID), LISTENING);
}

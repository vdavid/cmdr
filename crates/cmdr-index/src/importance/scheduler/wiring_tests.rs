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

/// Build an index DB for `volume_id` over the canonical synthetic home and route
/// the volume's read pool at it, so a spawned pass has something real to walk.
/// Without a pool, a pass reads nothing and writes nothing, and a test asserting
/// "no pass ran" would pass for the wrong reason.
fn install_index_for(data_dir: &std::path::Path, volume_id: &str) {
    let index_path = data_dir.join(format!("index-{volume_id}.db"));
    build_index_from_home(
        &index_path,
        &crate::importance::fixtures::SyntheticHome::canonical(1_000_000_000),
    );
    crate::indexing::read::enrichment::install_read_pool(
        volume_id,
        Arc::new(crate::indexing::read::enrichment::ReadPool::new(index_path).expect("read pool")),
    );
}

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
/// treadmill `docs/notes/importance-treadmill-2026-08-04.md` exists to document. Now
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

//! A pass stops with its volume, and a stopped pass can't be taken for a finished
//! one.
//!
//! Two halves, both deterministic (no thread races a walk, nothing sleeps):
//!
//! - **It really leaves early.** The walk and the scoring loop are stopped from
//!   INSIDE themselves, at a known point, and the tests count how much work ran
//!   afterwards. SQLite's progress handler is the seam for the walk: it fires on the
//!   walking thread every few VM instructions, in the middle of a row stream.
//! - **What it leaves behind.** The store a stopped pass wrote through is compared,
//!   row for row and stamp for stamp, against what it held before.
//!
//! The write's own rollback is pinned beside the writer
//! (`a_full_pass_stopped_mid_write_rolls_back_to_the_previous_pass`).

use std::sync::atomic::{AtomicUsize, Ordering};

use super::test_support::*;
use super::wiring::stop_hook_for;
use super::*;
use crate::importance::read::WeightsChanged;
use crate::importance::stop::STOP_CHECK_INTERVAL;
use crate::importance::store::{SCORING_POLICY_KEY, needs_full_pass, read_meta_value};
use crate::indexing::store::IndexStore;

/// A token that has already fired: a volume that stopped before the pass began.
fn already_stopped() -> CancellationToken {
    let stop = CancellationToken::new();
    stop.cancel();
    stop
}

/// Every weight row in the store, as `(path, score, signals, generation)`, in path
/// order: the whole of what a consumer can read, so two snapshots being equal means
/// the pass in between changed nothing.
fn stored_rows(dir: &std::path::Path, volume_id: &str) -> Vec<(String, f64, String, i64)> {
    let store = ImportanceStore::open(&importance_db_path(dir, volume_id)).expect("open store");
    let mut statement = store
        .read_conn()
        .prepare("SELECT path, score, signals, as_of_generation FROM weights ORDER BY path")
        .expect("prepare");
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("rows")
}

/// The two stamps a FINISHED full pass leaves: the generation, and whether the
/// scoring policy is this build's.
fn stamps(dir: &std::path::Path, volume_id: &str) -> (u64, bool) {
    let store = ImportanceStore::open(&importance_db_path(dir, volume_id)).expect("open store");
    let policy = read_meta_value(store.read_conn(), SCORING_POLICY_KEY).expect("meta");
    (
        store.recompute_generation().expect("generation"),
        policy.as_deref() == Some(crate::importance::classify::scoring_policy_fingerprint().as_str()),
    )
}

/// Full-pass `folders` through `writer` under `stop`.
fn full_pass(
    writer: &ImportanceWriter,
    home: &str,
    folders: &mut WalkedFolders,
    stop: &CancellationToken,
) -> Result<recompute::RecomputeOutcome, PassError> {
    recompute_folders(
        &RecomputeInputs {
            writer,
            weights: &Weights::default(),
            home,
            now_secs: 1_000_000_000,
            available: SignalSet::listing_only(),
            visits: &HashMap::new(),
            last_used: &HashMap::new(),
            stop,
        },
        folders,
    )
}

/// `count` plain user folders under `home`, none of which floors.
fn user_folders(home: &str, prefix: &str, count: usize) -> Vec<String> {
    (0..count).map(|i| format!("{home}/{prefix}/folder{i:05}")).collect()
}

// ── It really leaves early ────────────────────────────────────────────────

/// The red test of issue #230: start a walk over a large synthetic index, fire the
/// volume's signal while it is reading, and the walk leaves instead of reading on.
///
/// The progress handler runs on the walking thread every `TICK` SQLite VM
/// instructions, so it lands in the middle of a row stream, and it is deterministic:
/// the same index and the same queries take the same instructions. One complete walk
/// counts the ticks a whole walk takes; a second walk fires the signal halfway
/// through that count, and must end soon after rather than near the full count.
#[test]
fn a_walk_whose_volume_stops_mid_read_leaves_without_reading_the_rest() {
    /// VM instructions between two handler calls: a few rows' worth.
    const TICK: i32 = 200;

    let dir = tempfile::tempdir().expect("temp");
    let index_path = dir.path().join("index-root.db");
    // 5,102 folders and 30,600 files: tens of stop looks apart, so "stopped at the
    // next look" and "ran to the end" are far apart in the count.
    let home = build_folder_heavy_index(&index_path, 100, 50, 6);
    let store = IndexStore::open(&index_path).expect("reopen");
    let conn = store.read_conn();

    // How many ticks a whole walk takes.
    let ticks = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&ticks);
    conn.progress_handler(
        TICK,
        Some(move || {
            counted.fetch_add(1, Ordering::Relaxed);
            false
        }),
    )
    .expect("progress handler");
    let whole = walk_index_folders(conn, &home, &NEVER_STOPPED).expect("an unstopped walk finishes");
    assert_eq!(whole.len(), 5_102, "the unstopped walk found every folder");
    let whole_walk_ticks = ticks.load(Ordering::Relaxed);
    assert!(
        whole_walk_ticks > 1_000,
        "the index has to be big enough for 'mid-read' to mean something (tick count: {whole_walk_ticks})"
    );

    // Now stop the volume halfway through.
    let stop = CancellationToken::new();
    let fire = stop.clone();
    let stop_at = whole_walk_ticks / 2;
    let ticks = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&ticks);
    conn.progress_handler(
        TICK,
        Some(move || {
            if counted.fetch_add(1, Ordering::Relaxed) + 1 == stop_at {
                fire.cancel();
            }
            false
        }),
    )
    .expect("progress handler");

    let stopped = walk_index_folders(conn, &home, &stop);
    let ticks_run = ticks.load(Ordering::Relaxed);
    conn.progress_handler(0, None::<fn() -> bool>).expect("clear handler");

    assert_eq!(
        stopped.err(),
        Some(PassError::Cancelled),
        "a walk whose volume stopped reports it as a stop, never as a finished walk"
    );
    // One stop look covers `STOP_CHECK_INTERVAL` rows, and a row is a handful of VM
    // instructions, so the walk must end within a small fraction of what was left.
    let ran_after_the_stop = ticks_run - stop_at;
    let was_left = whole_walk_ticks - stop_at;
    assert!(
        ran_after_the_stop * 4 < was_left,
        "the walk read on after its volume stopped (tick count: {ran_after_the_stop}, with {was_left} left): \
         it has to leave at its next look, not finish the stream"
    );
}

/// Scoring stops at its next look too, and hands back NO rows: a partial row set
/// given to a full-pass write would replace the table with part of a volume.
#[test]
fn scoring_stops_at_its_next_look_and_returns_no_rows() {
    let home = "/Users/test";
    let total = 3 * STOP_CHECK_INTERVAL as usize;
    let paths = user_folders(home, "Documents", total);
    let path_refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let mut folders = WalkedFolders::synthetic(&path_refs, home);

    // The volume stops while folder 100 is being scored.
    let stop = CancellationToken::new();
    let mut scored = 0_usize;
    let rows = score_folders(
        &mut folders,
        home,
        &Weights::default(),
        &SignalSet::listing_only(),
        1_000_000_000,
        &stop,
        |_| {
            scored += 1;
            if scored == 100 {
                stop.cancel();
            }
            OptionalSignals::default()
        },
    );

    assert_eq!(rows.err(), Some(PassError::Cancelled));
    assert_eq!(
        scored, STOP_CHECK_INTERVAL as usize,
        "scoring ran to its next look and no further (folder count: {total})"
    );
}

// ── What a stopped pass leaves behind ─────────────────────────────────────

/// A stopped full pass leaves the store EXACTLY as the last finished pass left it:
/// the same rows at the same generation, the same two stamps. And the next pass
/// starts clean: it replaces the table and stamps the next generation, with no
/// trace of the stopped one (no skipped generation, no leftover row).
#[test]
fn a_stopped_full_pass_leaves_the_last_finished_pass_in_place() {
    let dir = tempfile::tempdir().expect("temp");
    let home = "/Users/test";
    let writer = ImportanceWriter::spawn(&importance_db_path(dir.path(), ROOT_VOLUME_ID)).expect("writer");

    // A finished pass over one set of folders.
    let before_paths = user_folders(home, "Documents", 40);
    let before_refs: Vec<&str> = before_paths.iter().map(String::as_str).collect();
    let finished = full_pass(
        &writer,
        home,
        &mut WalkedFolders::synthetic(&before_refs, home),
        &NEVER_STOPPED,
    )
    .expect("the first pass finishes");
    assert_eq!((finished.count, finished.generation), (40, 1));
    let rows_before = stored_rows(dir.path(), ROOT_VOLUME_ID);
    assert_eq!(rows_before.len(), 40);
    assert_eq!(stamps(dir.path(), ROOT_VOLUME_ID), (1, true));

    // The volume changed completely, and the pass over it is stopped.
    let after_paths = user_folders(home, "Pictures", 25);
    let after_refs: Vec<&str> = after_paths.iter().map(String::as_str).collect();
    let stopped = full_pass(
        &writer,
        home,
        &mut WalkedFolders::synthetic(&after_refs, home),
        &already_stopped(),
    );
    assert_eq!(stopped.err(), Some(PassError::Cancelled));
    assert_eq!(
        stored_rows(dir.path(), ROOT_VOLUME_ID),
        rows_before,
        "a stopped pass wrote nothing: every row of the last finished pass is still there, unchanged"
    );
    assert_eq!(
        stamps(dir.path(), ROOT_VOLUME_ID),
        (1, true),
        "and it stamped nothing: the generation and the policy are the finished pass's"
    );

    // The same pass, run again once the volume is back.
    let redone = full_pass(
        &writer,
        home,
        &mut WalkedFolders::synthetic(&after_refs, home),
        &NEVER_STOPPED,
    )
    .expect("the next pass finishes");
    assert_eq!(
        (redone.count, redone.generation),
        (25, 2),
        "the next pass takes the very next generation: the stopped one consumed none"
    );
    let rows_after = stored_rows(dir.path(), ROOT_VOLUME_ID);
    assert_eq!(rows_after.len(), 25, "and it replaced the table whole");
    assert!(rows_after.iter().all(|row| row.0.contains("/Pictures/") && row.3 == 2));

    writer.shutdown();
}

/// What makes a stopped pass get REDONE: nothing records it. The "this store needs a
/// full pass" probe (`needs_full_pass`, which `wire_volume` runs for every volume it
/// wires) keys on the generation and policy stamps, and only a finished pass writes
/// them. So a first pass that was stopped is still owed the next time the volume is
/// wired, and one that finished isn't.
#[test]
fn a_stopped_first_pass_is_still_owed() {
    let dir = tempfile::tempdir().expect("temp");
    let home = "/Users/test";
    let writer = ImportanceWriter::spawn(&importance_db_path(dir.path(), ROOT_VOLUME_ID)).expect("writer");
    let paths = user_folders(home, "Documents", 10);
    let path_refs: Vec<&str> = paths.iter().map(String::as_str).collect();

    let stopped = full_pass(
        &writer,
        home,
        &mut WalkedFolders::synthetic(&path_refs, home),
        &already_stopped(),
    );
    assert_eq!(stopped.err(), Some(PassError::Cancelled));
    assert!(stored_rows(dir.path(), ROOT_VOLUME_ID).is_empty());
    assert!(
        needs_full_pass(dir.path(), ROOT_VOLUME_ID).expect("probe"),
        "a stopped first pass leaves the store reading as never scored, so the next wiring redoes it"
    );

    full_pass(
        &writer,
        home,
        &mut WalkedFolders::synthetic(&path_refs, home),
        &NEVER_STOPPED,
    )
    .expect("the redone pass finishes");
    assert!(
        !needs_full_pass(dir.path(), ROOT_VOLUME_ID).expect("probe"),
        "a finished pass is what settles it"
    );

    writer.shutdown();
}

/// The whole pass, through the scheduler: a stopped pass tells no consumer that
/// weights changed, and the next one does. A consumer that reloaded on a stopped
/// pass would be reloading a store that didn't move; one that was told a generation
/// landed when it didn't would be wrong about what it holds.
#[test]
fn a_stopped_pass_announces_nothing_and_the_next_one_does() {
    const VOLUME: &str = "stop-tests-full-pass";
    let dir = tempfile::tempdir().expect("temp");
    install_index_for(dir.path(), VOLUME);
    let scheduler = ImportanceScheduler::new(dir.path().to_path_buf());
    let mut notices = crate::importance::read::subscribe(VOLUME);

    // The volume is wired, then stops (its root token fires, and the child with it).
    let volume_root = CancellationToken::new();
    scheduler.adopt_stop(VOLUME, volume_root.child_token());
    volume_root.cancel();

    let stopped = scheduler.run_pass_under_current_stop(VOLUME, SignalSet::listing_only(), 1_000_000_000);
    assert_eq!(stopped, Err(PassError::Cancelled));
    assert!(notices.try_recv().is_err(), "a stopped pass announces nothing");
    assert!(
        needs_full_pass(dir.path(), VOLUME).expect("probe"),
        "and the store still reads as unscored"
    );

    // The volume starts again: it is wired again, with a child of its NEW root.
    scheduler.adopt_stop(VOLUME, CancellationToken::new().child_token());
    let scored = scheduler
        .run_pass_under_current_stop(VOLUME, SignalSet::listing_only(), 1_000_000_000)
        .expect("the pass finishes once the volume is back");
    assert!(scored > 0);
    assert!(
        matches!(notices.try_recv(), Ok(WeightsChanged::ReloadAll { generation: 1 })),
        "the finished pass announces the generation it stamped"
    );
    assert_eq!(stamps(dir.path(), VOLUME), (1, true));

    crate::indexing::read::enrichment::uninstall_read_pool(VOLUME);
}

/// A stopped incremental rescore writes nothing, announces nothing, and KEEPS its
/// batch: the changed folders are still owed a rescore, so they go back into the
/// pending set for the volume's next one instead of being counted as handled.
#[test]
fn a_stopped_rescore_keeps_its_batch_for_the_next_one() {
    const VOLUME: &str = "stop-tests-rescore";
    let dir = tempfile::tempdir().expect("temp");
    install_index_for(dir.path(), VOLUME);
    let scheduler = ImportanceScheduler::new(dir.path().to_path_buf());
    let home = ImportanceScheduler::home_dir();
    // Unfloored wherever `$HOME` points, so the batch gate lets it through.
    let batch = vec![format!("{home}/importance-stop-tests/changed")];
    let mut notices = crate::importance::read::subscribe(VOLUME);

    scheduler.adopt_stop(VOLUME, already_stopped());
    let stopped =
        scheduler.run_incremental_under_current_stop(VOLUME, SignalSet::listing_only(), batch.clone(), 1_000_000_000);

    assert_eq!(stopped, Err(PassError::Cancelled));
    assert!(notices.try_recv().is_err(), "a stopped rescore announces nothing");
    assert_eq!(
        scheduler.take_incremental_paths(VOLUME),
        batch,
        "the batch is still pending, so the next rescore folds it in"
    );

    crate::indexing::read::enrichment::uninstall_read_pool(VOLUME);
}

// ── Whose signal a pass hears ─────────────────────────────────────────────

/// The scheduler's stop hook (what `stop_all_indexing` runs for the memory watchdog
/// and the master switch) fires every wired volume's signal at once, and a volume
/// stays stopped until it is wired again with a fresh signal.
#[test]
fn the_stop_hook_stops_every_wired_volume_until_it_is_wired_again() {
    let dir = tempfile::tempdir().expect("temp");
    let scheduler = Arc::new(ImportanceScheduler::new(dir.path().to_path_buf()));
    scheduler.adopt_stop("hook-a", CancellationToken::new());
    scheduler.adopt_stop("hook-b", CancellationToken::new());
    assert!(!scheduler.stop_for("hook-a").is_cancelled());

    let hook = stop_hook_for(&scheduler);
    hook();
    assert!(scheduler.stop_for("hook-a").is_cancelled());
    assert!(scheduler.stop_for("hook-b").is_cancelled());

    // A token is one-shot: the volume comes back by being wired again.
    scheduler.adopt_stop("hook-a", CancellationToken::new());
    assert!(!scheduler.stop_for("hook-a").is_cancelled());
    assert!(scheduler.stop_for("hook-b").is_cancelled(), "and only that volume");

    // The hook outlives the scheduler in the process-wide registry, harmlessly.
    drop(scheduler);
    hook();
}

/// A volume nothing wired has no signal to hear, so its pass doesn't start: it reads
/// as already stopped instead of running under a token that could never fire.
#[test]
fn a_volume_nothing_wired_reads_as_stopped() {
    let dir = tempfile::tempdir().expect("temp");
    let scheduler = ImportanceScheduler::new(dir.path().to_path_buf());
    assert!(scheduler.stop_for("never-wired").is_cancelled());
    assert_eq!(
        scheduler.run_pass_under_current_stop("never-wired", SignalSet::listing_only(), 1_000_000_000),
        Err(PassError::Cancelled)
    );
}

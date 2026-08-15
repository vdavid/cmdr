//! What a covered volume owes, and in which order it owes it.
//!
//! Completion is a pure function of the database ("the frontier under this root is
//! empty"), so these pin both halves: ground no walk could read has to LEAVE the
//! frontier, and the terminal reports have to sit either side of the flush that
//! the final aggregate runs inside. Swapping the last two is a shipped bug, not a
//! tidy-up: it leaves every hourglass lit until the next launch.

use super::*;

/// The bounded-progress rule, and the reason completion can be a pure function of
/// the database. A directory a walk gave up on is never marked listed, so on its
/// own it would sit in the frontier forever and NOTHING hanging off completion
/// would ever happen: the stamps, the ledger heal, the sweep keys, the branch
/// collapse, the media kick, freshness. The cause is what takes it out.
///
/// Staged the way a walk stages it — the same `MarkDirsUnreadable` message the
/// walker sends — because what is under test is the completion rule, not the three
/// producers of the cause.
#[test]
fn a_permanently_timing_out_directory_still_lets_completion_happen() {
    completion_survives("phased-timed-out", crate::indexing::store::UnreadableCause::Abandoned);
}

/// The same rule for the other shape it arrives in: a subtree the walker's
/// consecutive-failure budget pruned without reading. It carries the same cause
/// and has to behave the same way.
#[test]
fn a_subtree_pruned_by_the_failure_budget_still_lets_completion_happen() {
    completion_survives("phased-pruned", crate::indexing::store::UnreadableCause::Abandoned);
}

fn completion_survives(volume_id: &'static str, cause: crate::indexing::store::UnreadableCause) {
    let drive = Drive::new(
        volume_id,
        |root| {
            std::fs::create_dir_all(root.join("readable")).expect("dirs");
            std::fs::create_dir_all(root.join("unreadable/inside")).expect("dirs");
        },
        &[],
    );
    // Stitch the tree root so `unreadable` has a row, then condemn it exactly as a
    // walk that couldn't read it would.
    drive.start();
    drive.wait_for_the_machine();
    let id = drive
        .id_of(&drive.path("unreadable"))
        .expect("the walk wrote a row for it");
    drive.write(WriteMessage::MarkDirsUnreadable { ids: vec![id], cause });
    drive.write(WriteMessage::MarkDirsListed {
        ids: vec![id],
        epoch: 0,
    });
    {
        let conn = IndexStore::open_write_connection(&drive.db_path()).expect("write connection");
        IndexStore::delete_meta(&conn, "scan_completed_at").expect("a partial index, as a relaunch would find it");
    }

    drive.restart();
    drive.wait_for_the_machine();

    assert!(
        drive.frontier(&drive.path("")).is_empty(),
        "ground no walk can read is not frontier, so it doesn't hold completion open"
    );
    assert!(
        drive.meta("scan_completed_at").is_some(),
        "and the volume completes with a hole in it, which is the honest answer"
    );
}

/// Everything a completed volume owes that nothing else would ever do. The
/// `dir_stats` ledger heal is armed on every launch and disarmed only by a full
/// `ComputeAllAggregates`, which cover walks never send; the sweep ledger is seeded
/// from meta only at launch, so without these keys the first shallow anchor after
/// completion triggers a full sweep nobody asked for.
#[test]
fn completion_pays_the_ledger_and_seeds_the_sweep_keys() {
    let drive = Drive::new(
        "phased-completion-owes",
        |root| {
            std::fs::create_dir_all(root.join("one")).expect("dirs");
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();

    assert!(
        drive
            .meta(crate::indexing::reconcile::reconciler::SHALLOW_SWEEP_AT_KEY)
            .is_some(),
        "the sweep window starts now, or the next shallow anchor sweeps the whole drive"
    );
    assert_eq!(
        drive
            .meta(crate::indexing::reconcile::reconciler::SHALLOW_COALESCED_KEY)
            .as_deref(),
        Some("0"),
        "and the coalesced count starts over"
    );
    let conn = IndexStore::open_read_connection(&drive.db_path()).expect("read connection");
    assert!(
        IndexStore::ledger_heal_done(&conn).expect("the ledger marker"),
        "`PayLedgerIfUnpaid` is the only thing that ever pays the armed heal"
    );
    assert!(
        drive.meta("total_entries").is_some_and(|value| value != "0"),
        "and the calibration a later run's ETA is built from is recorded"
    );
}

/// The sequence fires on the absent→present transition and never again. The
/// machine takes stock after EVERY drain, so a missing guard would re-run it
/// several times in one run — rewriting the sweep keys and pushing the 24-hour
/// window forward each time, which is the mirror of the bug the sweep ledger
/// exists to prevent.
#[test]
fn the_completion_sequence_runs_once_however_often_the_machine_takes_stock() {
    let drive = Drive::new(
        "phased-completes-once",
        |root| {
            // Several frontier roots, so the run drains and takes stock repeatedly.
            for name in ["a", "b", "c", "d"] {
                std::fs::create_dir_all(root.join(name).join("inner")).expect("dirs");
            }
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();
    drive.meta("scan_completed_at").expect("it completed");

    assert_eq!(
        drive
            .events
            .kinds_for(drive.volume_id)
            .iter()
            .filter(|kind| **kind == crate::indexing::events::IndexEventKind::ScanComplete)
            .count(),
        1,
        "one completion per run, however many times the machine asked the question"
    );

    // And a relaunch of a COMPLETED volume never reaches the machine at all: it
    // takes today's reconcile-in-place path, which leaves the rows standing.
    //
    // ⚠️ Waited on the DURABLE marker, ❌ not on `scanning`. That path clears the
    // marker before it walks and re-stamps at the end, and its completion handler
    // drops the flag BEFORE the meta write reaches the writer — a window Linux
    // loses regularly.
    let rows = drive.entry_count();
    drive.restart();
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(30),
        "the relaunch to record its own completion",
        || drive.meta("scan_completed_at").is_some(),
    );
    assert!(drive.entry_count() >= rows, "and it never blanks the index");
}

/// **What the user is left looking at.** The completion sequence queues the
/// `dir_stats` ledger heal, and its full `ComputeAllAggregates` streams progress
/// for as long as it runs (18.8 s over a real `/`). A status surface reopens on a
/// progress tick and only a terminal event closes it, so a terminal fired BEFORE
/// that aggregate leaves the corner hourglass, every folder row's size hourglass,
/// and the step checklist lit for the rest of the session.
#[test]
fn nothing_aggregates_after_the_volume_says_aggregation_is_done() {
    use crate::indexing::events::IndexEventKind;

    let drive = Drive::new(
        "phased-terminal-aggregation",
        |root| {
            for name in ["a", "b"] {
                std::fs::create_dir_all(root.join(name).join("inner")).expect("dirs");
            }
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();

    let kinds = drive.events.kinds_for(drive.volume_id);
    let terminal = kinds
        .iter()
        .position(|kind| *kind == IndexEventKind::AggregationComplete)
        .expect("a completed volume reports aggregation as done");
    let last_tick = kinds
        .iter()
        .rposition(|kind| *kind == IndexEventKind::AggregationProgress)
        .expect("the ledger heal really does stream progress, or this test proves nothing");

    assert!(
        last_tick < terminal,
        "the terminal event is the LAST word on aggregation: a tick after it reopens a step \
         nothing closes again (last tick at {last_tick}, terminal at {terminal})"
    );
}

/// The phase header is the ORDER made visible, and the order is the whole
/// feature. A folder the user opens mid-run is covered as its own phase and
/// announces itself, so without a re-assertion afterwards the header names that
/// interlude for the rest of the run — "Indexing the folders you use most" while
/// the machine is actually walking the whole drive.
#[test]
fn the_outer_phase_says_so_again_after_a_visited_root_interrupts_it() {
    use crate::indexing::events::CoveragePhase;

    let drive = Drive::with_host(
        "phased-phase-reasserts",
        |root| {
            for name in ["a", "b", "zzz-visited"] {
                std::fs::create_dir_all(root.join(name).join("inner")).expect("dirs");
            }
        },
        |host, root| {
            host.note_open_listing("phased-phase-reasserts", root.join("zzz-visited"));
        },
        &[],
        true,
    );
    drive.start();
    drive.wait_for_the_machine();

    let announced = drive.announced_phases();

    assert!(
        announced.contains(&CoveragePhase::VisitedRoot),
        "the folder the user is looking at really did earn a phase of its own, \
         or there is no interlude here to re-assert after ({announced:?})"
    );
    assert_eq!(
        announced.last(),
        Some(&CoveragePhase::WholeVolume),
        "and the last thing announced is what the machine is actually walking ({announced:?})"
    );
}

/// The early signal, and its blast radius. Photo search and folder importance only
/// need `$HOME`; waiting for the rest of the drive is minutes of the most visibly
/// valuable feature sitting idle on a first run. ⚠️ It says nothing about
/// freshness: the volume genuinely isn't covered yet.
#[test]
fn home_coverage_fires_the_early_media_signal_without_claiming_fresh() {
    let drive = Drive::new(
        "phased-home-signal",
        |root| {
            std::fs::create_dir_all(root.join("home/Documents")).expect("dirs");
            // The rest of the drive, which the signal must not wait for.
            std::fs::create_dir_all(root.join("zzz-elsewhere/deep")).expect("dirs");
        },
        &[],
    );
    let _home = set_home_override(drive.tree.path().join("home"));
    let mut signal = crate::indexing::lifecycle::lifecycle_bus::subscribe_home_covered(drive.volume_id);

    drive.start();
    cmdr_fs::testing::wait_until(std::time::Duration::from_secs(30), "home to read as covered", || {
        drive.meta(HOME_COVERED_AT_KEY).is_some()
    });

    assert!(*signal.borrow_and_update(), "the one subscriber hears about it");
    drive.wait_for_the_machine();
    assert!(
        drive.meta("scan_completed_at").is_some(),
        "and the drive still completes on its own rule afterwards"
    );
}

/// The admission the early kick lives or dies on. `ready_volumes_with_kind` only
/// WIRES subscriptions, and a relaunch mid-coverage finds a volume that is
/// home-covered but not `Fresh` — so without this the early kick works on the first
/// run and never again.
#[test]
fn a_relaunch_mid_coverage_still_wires_the_media_subscriptions() {
    let drive = Drive::new(
        "phased-relaunch-wires",
        |root| {
            std::fs::create_dir_all(root.join("home/Documents")).expect("dirs");
        },
        &[],
    );
    let _home = set_home_override(drive.tree.path().join("home"));
    drive.start();
    drive.wait_for_the_machine();

    // What a relaunch mid-coverage looks like: home covered, the drive not.
    {
        let conn = IndexStore::open_write_connection(&drive.db_path()).expect("write connection");
        IndexStore::delete_meta(&conn, "scan_completed_at").expect("clear the completion marker");
    }
    crate::indexing::lifecycle::state::apply_freshness_event(
        drive.volume_id,
        crate::indexing::lifecycle::freshness::FreshnessEvent::ScanStarted,
    );

    let ready = crate::indexing::lifecycle::state::ready_volumes_with_kind();
    assert!(
        ready.iter().any(|(vid, _)| vid == drive.volume_id),
        "a home-covered volume is admitted even though it is honestly not Fresh: {ready:?}"
    );
}

//! What a second launch finds, and what the two switches do to a half-covered
//! drive.
//!
//! Every case here answers one question: does this volume ADD to what the last
//! session bought, or throw it away? The launch table reads the persisted branch
//! set to tell a phased partial from an interrupted bulk scan, so a rescan, a
//! master-switch cycle, a stop, and a missing journal must each land on the
//! resuming side of it, and the escape hatch on the rebuilding side.

use super::*;

/// Both switches keep outranking everything, and they are asked per phase and per
/// root rather than only at launch — so turning drive indexing off stops the
/// walking rather than the next launch.
#[test]
fn master_off_runs_nothing() {
    // The handle's own guard puts the process-wide switch back when the fixture
    // drops, so this can't leak into another test.
    let drive = Drive::with_host(
        "phased-master-off",
        |root| {
            std::fs::create_dir_all(root.join("one")).expect("dirs");
        },
        |_, _| {},
        &[],
        false,
    );

    drive.start();

    assert!(
        drive.meta("scan_completed_at").is_none(),
        "nothing indexes when the master switch is off"
    );
    assert_eq!(drive.scans_started(), 0, "and no run is announced");
}

/// A stopped machine leaves what it covered covered, and a restart adds to it
/// rather than starting over. This is the property today's truncate-and-rebuild
/// first scan can't have, and the first reason the whole design exists.
#[test]
fn rows_survive_a_stopped_and_restarted_machine() {
    let drive = Drive::new(
        "phased-survives-restart",
        |root| {
            for name in ["a", "b", "c"] {
                std::fs::create_dir_all(root.join(name).join("inner")).expect("dirs");
            }
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();
    let after_first = drive.entry_count();
    let epoch = drive.current_epoch();
    assert!(after_first > 1, "precondition: the first run wrote rows");

    // What a quit mid-coverage leaves behind: rows, and no completion marker.
    {
        let conn = IndexStore::open_write_connection(&drive.db_path()).expect("write connection");
        IndexStore::delete_meta(&conn, "scan_completed_at").expect("clear the completion marker");
    }
    drive.restart();
    drive.wait_for_the_machine();

    assert!(
        drive.entry_count() >= after_first,
        "a restart adds to the index; ❌ it never blanks it"
    );
    assert!(
        drive.meta("scan_completed_at").is_some(),
        "the resumed run confirms what is covered and stamps it"
    );
    let _ = epoch;
}

// ── What a launch does with the index it finds ────────────────────────

/// The routing table, end to end, over the two shapes that look identical from a
/// row count: rows and no completion marker, told apart by whether anything
/// records which ground those rows cover.
///
/// The pure table itself is `manager/launch_route.rs`; these two run the real
/// path and check what actually happened to the database.
#[test]
fn a_stopped_phased_index_comes_back_with_its_rows() {
    let drive = Drive::new(
        "phased-resume-keeps-rows",
        |root| {
            std::fs::create_dir_all(root.join("covered/inner")).expect("dirs");
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();

    drive.stop();
    drive.plant_a_ghost("covered/inner", "last-session.txt");
    drive.forget_the_completion_marker();
    drive.start();
    drive.wait_for_the_machine();

    assert!(
        drive.ghost_survived("covered/inner", "last-session.txt"),
        "❌ a partially covered volume must come back as a partially covered volume: the machine ADDS \
         to what the last session bought, it never throws it away"
    );
    assert!(
        drive.meta("scan_completed_at").is_some(),
        "and the resumed run confirms what is covered and stamps it"
    );
}

/// Its opposite number. The same rows, minus the record of which ground they
/// cover, is a first BULK scan somebody interrupted — and nothing can watch that
/// ground or mark it stale, so resuming into it would render last session's sizes
/// as CURRENT with nothing having verified them. It goes.
#[test]
fn a_legacy_interrupted_partial_is_thrown_away_and_rebuilt() {
    let drive = Drive::new(
        "phased-legacy-partial",
        |root| {
            std::fs::create_dir_all(root.join("covered/inner")).expect("dirs");
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();

    drive.stop();
    drive.plant_a_ghost("covered/inner", "last-session.txt");
    drive.forget_the_completion_marker();
    drive.forget_the_covered_branches();
    drive.start();
    drive.wait_for_the_machine();

    assert!(
        !drive.ghost_survived("covered/inner", "last-session.txt"),
        "❌ rows nothing can account for are rebuilt, not walked on top of"
    );
    assert!(
        drive.frontier(&drive.path("")).is_empty(),
        "and the rebuilt index converges, by the same machine"
    );
    assert!(
        drive.meta("scan_completed_at").is_some(),
        "all the way to its completion marker"
    );
}

/// Turning drive indexing off and back on used to be a truncate door: the resume
/// always went to `start_scan`, which reads rows-without-a-completion-marker as a
/// partial that never finished and blanks it.
///
/// The drive also has to be OFFERED the resume, which is the second half here: an
/// external drive part way through its first index has no completion marker, so
/// `drives_to_resume` used to leave it behind and the switch coming back on was
/// where a half-built index went to be forgotten. The enable the user gave it is
/// what brings it back.
#[test]
fn a_master_switch_cycle_resumes_the_phases_instead_of_rebuilding() {
    let drive = Drive::new(
        "phased-master-cycle",
        |root| {
            std::fs::create_dir_all(root.join("covered/inner")).expect("dirs");
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();

    // Off stops every running index, exactly as the settings switch does.
    drive.index.set_indexing_enabled(false);
    drive.plant_a_ghost("covered/inner", "last-session.txt");
    drive.forget_the_completion_marker();

    drive.index.set_indexing_enabled(true);
    assert!(
        drive.index.drives_to_resume().iter().any(|id| id == drive.volume_id),
        "the switch coming back on must offer the drive the user turned on, unfinished first index and all"
    );
    drive.start();
    drive.wait_for_the_machine();

    assert!(
        drive.ghost_survived("covered/inner", "last-session.txt"),
        "❌ the master switch coming back on must not cost the drive what it already covered"
    );
}

/// The AUTOMATIC rescan door: a coalesced shallow `MustScanSubDirs`, a replay that
/// couldn't roll forward, an ingestion backlog. All three land on
/// `perform_registry_rescan`, which used to call `start_scan` unconditionally — so
/// any of them could blank a half-built index without a person involved.
///
/// (A branch-confined volume never routes a shallow anchor here at all, which is
/// its own guard and its own test in `../../reconcile/reconciler/tests/`. This one
/// says the door is closed even when something does reach it.)
#[test]
fn an_automatic_rescan_restarts_the_phases_instead_of_truncating() {
    let drive = Drive::new(
        "phased-automatic-rescan",
        |root| {
            std::fs::create_dir_all(root.join("covered/inner")).expect("dirs");
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();

    drive.stop();
    drive.plant_a_ghost("covered/inner", "last-session.txt");
    // Cleared before the restart too, or the resume takes the COMPLETED route —
    // a reconcile in place, which re-lists the tree and would take the ghost with
    // it before this test got to its own question.
    drive.forget_the_completion_marker();
    drive.start();
    drive.wait_for_the_machine();

    // A volume the machine hasn't finished, which is the state the door is
    // dangerous in: rows, and no completion marker to stop `start_scan` reading
    // them as a partial to blank.
    drive.forget_the_completion_marker();
    let epoch = drive.current_epoch();
    crate::indexing::host::runtime::block_on(crate::indexing::lifecycle::manager::perform_registry_rescan(
        drive.volume_id,
        "ingestion backlog",
    ));
    drive.wait_for_the_machine();

    assert_eq!(
        drive.current_epoch(),
        epoch,
        "❌ no truncating (re)scan ran: every one of them bumps the epoch before it walks"
    );
    assert!(
        drive.ghost_survived("covered/inner", "last-session.txt"),
        "❌ and the rows the machine had already covered are still there"
    );
    // The door stops the watcher and the live loop on its way in, expecting the
    // full scan it used to reach to start fresh ones. This volume's frontier is
    // already empty, so no walk runs and nothing else would put one back.
    assert!(
        crate::indexing::lifecycle::state::is_watching_for_test(drive.volume_id),
        "❌ covered ground the drive still serves must not be left with nothing watching it"
    );
}

/// The escape hatch's own row, which is the one nobody would write down. With the
/// switch off there is no phase machine to resume into, so a phased partial takes
/// today's truncating rebuild — self-healing, and what the person who flipped it
/// asked for.
#[test]
fn the_kill_switch_gives_a_phased_partial_back_to_the_bulk_scan() {
    let drive = Drive::new(
        "phased-kill-switch",
        |root| {
            std::fs::create_dir_all(root.join("covered/inner")).expect("dirs");
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();

    drive.stop();
    drive.plant_a_ghost("covered/inner", "last-session.txt");
    drive.forget_the_completion_marker();

    // Off, exactly as a launch would find it. Restored when the guard drops.
    let _killed = crate::indexing::lifecycle::phases::install_for_test(false);
    drive.start();
    cmdr_fs::testing::wait_until(std::time::Duration::from_secs(30), "the bulk scan to finish", || {
        drive.meta("scan_completed_at").is_some()
    });

    assert!(
        !drive.ghost_survived("covered/inner", "last-session.txt"),
        "the bulk build truncates first, which is the behavior the switch restores"
    );
    assert!(
        drive.frontier(&drive.path("")).is_empty(),
        "and the drive is indexed, by the path that shipped before the phases"
    );
}

/// The resume-honesty property, and the reason the machine's first walk waits for
/// `resume_branch_watch`. A session that can't replay the gap since the last one
/// doesn't know what happened to the ground it covered — so the rows stay
/// (Decision 5: a covered-but-stale subtree is not re-walked) and the epoch bump is
/// what makes the read side RENDER them as stale instead of confidently current.
///
/// ⚠️ Start the machine before that resume and the bump never fires: its first walk
/// starts a watcher, and `ensure_branch_watch` returns early when one is already
/// running.
#[test]
fn a_relaunch_with_no_replayable_journal_bumps_the_epoch() {
    let drive = Drive::new(
        "phased-resume-honesty",
        |root| {
            std::fs::create_dir_all(root.join("covered")).expect("dirs");
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();
    let before = drive.current_epoch();
    let rows = drive.entry_count();

    {
        let conn = IndexStore::open_write_connection(&drive.db_path()).expect("write connection");
        IndexStore::delete_meta(&conn, "scan_completed_at").expect("clear the completion marker");
    }
    drive.restart();
    drive.wait_for_the_machine();

    assert!(
        drive.current_epoch() > before,
        "a session that can't replay the gap says so, rather than claiming the rows are current"
    );
    assert!(
        drive.entry_count() >= rows,
        "and it says it by bumping the epoch, ❌ never by throwing the rows away"
    );
}

//! The truncating rescan a live walk refuses, and the promise that refusal
//! makes: remembered on a completed drive, fired by the walk that blocked it,
//! and re-asked against every walk still holding ground.

use super::*;

/// The refusal keeps its promise: a rescan asked for while a walk holds ground is
/// REMEMBERED, and the walk's own ending runs it.
///
/// A walk lasts seconds to minutes, so "ask again later" is a burden on the one
/// person who can't tell when later is. The user clicked a button that says
/// "Rescan now"; the button owes them a scan.
///
/// ⚠️ On an INDEXED drive, which is the mechanism's whole domain: a drive with no
/// completed scan is the phase machine's, and a rescan there restarts the phases
/// straight away rather than waiting for anything
/// (`a_rescan_during_the_phased_window_starts_the_machine_under_a_live_walk`).
#[test]
fn a_rescan_refused_under_a_walk_runs_when_the_walk_ends() {
    let drive = ColdDrive::new("cover-deferred-rescan-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");
    std::fs::write(drive.tree.path().join("scope/found.txt"), "x").expect("file");
    let scope = drive.path("scope");

    drive.cover(&scope);
    drive.mark_scan_completed();

    // The ground a walk holds for as long as it runs, taken directly so the window
    // is deterministic rather than a race against a walk of two files.
    let claim = Claim::take(drive.volume_id, vec![scope.clone()], Mode::Additive);
    crate::indexing::lifecycle::state::begin_branch_coverage(drive.volume_id, claim.mine());

    assert_eq!(
        crate::indexing::lifecycle::state::force_scan(drive.volume_id),
        Ok(RescanOutcome::DeferredUntilSearchEnds),
        "the rescan can't run under the walk, and says so as a variant rather than a sentence"
    );
    assert_eq!(drive.scans_started(), 0, "so nothing truncates while the walk writes");

    // The walk ends, through the one path `cover::start`'s thread ends one with.
    release_ground(drive.volume_id, claim);

    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(10),
        "the remembered rescan to run once the ground is free",
        || drive.scans_started() == 1,
    );
}

/// The remembered request waits for the LAST walk out, not the first.
///
/// Two searches can hold different ground on one volume, and the first to finish
/// frees only its own. Firing on that one would truncate under the other, which is
/// the whole bug the refusal exists to prevent — so the guard is asked again at
/// the moment the scan would start, and a refusal there re-remembers the request.
#[test]
fn a_remembered_rescan_waits_for_the_last_walk_out() {
    let drive = ColdDrive::new("cover-deferred-rescan-two-walks-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");
    std::fs::create_dir_all(drive.tree.path().join("elsewhere")).expect("dirs");
    let scope = drive.path("scope");
    drive.cover(&scope);
    drive.mark_scan_completed();

    let first = Claim::take(drive.volume_id, vec![scope.clone()], Mode::Additive);
    let second = Claim::take(drive.volume_id, vec![drive.path("elsewhere")], Mode::Additive);
    crate::indexing::lifecycle::state::begin_branch_coverage(drive.volume_id, first.mine());

    assert_eq!(
        crate::indexing::lifecycle::state::force_scan(drive.volume_id),
        Ok(RescanOutcome::DeferredUntilSearchEnds),
    );

    // The first walk ends. Run the fire on this thread rather than through the
    // spawn, so the assertion below is about the decision and not about timing.
    crate::indexing::lifecycle::state::finish_branch_coverage(drive.volume_id, first.mine());
    drop(first);
    crate::indexing::lifecycle::rescan_request::run_owed_now(drive.volume_id);
    assert_eq!(
        drive.scans_started(),
        0,
        "the second walk still holds ground, so nothing truncates under it"
    );

    drop(second);
    crate::indexing::lifecycle::rescan_request::run_owed_now(drive.volume_id);
    assert_eq!(drive.scans_started(), 1, "and the last walk out runs the scan");
}

/// A drive the user turned indexing off for is owed nothing, however the request
/// got there.
///
/// The request lives outside the registry (a walk ending has to reach it whatever
/// phase the volume is in), so it needs its own tie to teardown; without one, the
/// walk in flight would rescan a drive nobody is indexing any more.
#[test]
fn a_drive_that_stopped_indexing_is_owed_no_rescan() {
    let drive = ColdDrive::new("cover-deferred-rescan-teardown-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");
    let scope = drive.path("scope");
    drive.cover(&scope);
    drive.mark_scan_completed();

    let walking = Claim::take(drive.volume_id, vec![scope.clone()], Mode::Additive);
    assert_eq!(
        crate::indexing::lifecycle::state::force_scan(drive.volume_id),
        Ok(RescanOutcome::DeferredUntilSearchEnds),
    );

    drive
        .index
        .disable_volume(drive.volume_id)
        .expect("turn indexing off for the drive");
    assert!(
        !take_rescan(drive.volume_id),
        "the request went with the index it was made against, so re-enabling the drive later \
         doesn't inherit a scan nobody asked for"
    );

    drop(walking);
    crate::indexing::lifecycle::rescan_request::run_owed_now(drive.volume_id);
    assert_eq!(
        drive.scans_started(),
        0,
        "the walk ended, and the drive that stopped indexing stays stopped"
    );
}

/// The other direction of the same rule, and the sharper one: a volume a walk is
/// covering isn't TRUNCATED under it.
///
/// `start_scan` takes the whole volume as a claim, and a walk holding any of it
/// refuses that claim. Without it, a rescan through any door (the manual button, a
/// journal-gap fallback, a coalesced shallow anchor) sends `TruncateData` +
/// `BumpCurrentEpoch` while the walk is still writing: the walk's rows land in a
/// database that was blanked underneath them, and everything it attributed to an
/// id the truncate dropped is orphaned.
#[test]
fn a_truncating_rescan_refuses_while_a_search_cover_walk_is_live() {
    let drive = ColdDrive::new("cover-truncate-guard-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");
    std::fs::write(drive.tree.path().join("scope/found.txt"), "x").expect("file");
    let scope = drive.path("scope");

    drive.cover(&scope);
    assert!(
        drive.is_indexed(&drive.path("scope/found.txt")),
        "precondition: the walk's rows are in"
    );
    // An INDEXED drive, which is what makes "Rescan now" here a full (re)scan and
    // so a truncate risk at all. The never-scanned shape is the phase machine's,
    // and the test below covers it.
    drive.mark_scan_completed();
    let epoch = drive.current_epoch();

    // The hold a walk keeps for as long as it runs, taken directly so the window
    // is deterministic rather than a race against a walk of two files.
    let walking = Claim::take(drive.volume_id, vec![scope.clone()], Mode::Additive);

    assert_eq!(
        crate::indexing::lifecycle::state::force_scan(drive.volume_id),
        Ok(RescanOutcome::DeferredUntilSearchEnds),
        "a rescan waits for the walk holding ground on the volume"
    );
    assert_eq!(
        drive.scans_started(),
        0,
        "so no scan announced itself, which is also where the truncate would have been sent from"
    );
    assert_eq!(
        drive.current_epoch(),
        epoch,
        "and the epoch the walk's rows carry is untouched: a bump under a live walk renders \
         everything it just wrote as stale"
    );

    drop(walking);
    assert_eq!(
        crate::indexing::lifecycle::state::force_scan(drive.volume_id),
        Ok(RescanOutcome::Started),
        "and the moment the walk ends the rescan runs"
    );
}

/// The other half of the same claim: a rescan asked for while a full scan owns
/// the volume QUEUES behind it, and that scan's ending runs it.
///
/// The person clicking "Rescan now" mid-scan wants the drive as it is NOW, and the
/// scan already running was started against a state they've moved past. Answering
/// `Started` reported success for work nobody was going to do, which is the one
/// outcome a button must never produce. So the second walk really does run, and the
/// user is told it's waiting rather than done.
#[test]
fn a_rescan_asked_for_during_a_scan_runs_when_that_scan_ends() {
    let drive = ColdDrive::new("cover-rescan-under-scan-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");

    drive.cover(&drive.path("scope"));
    drive.mark_scan_completed();

    // The ground a full scan holds for as long as it runs: the whole volume,
    // taken the way `start_scan` takes it.
    let volume_root = drive.tree.path().to_string_lossy().to_string();
    let scanning = Claim::take(drive.volume_id, vec![volume_root], Mode::Exclusive);

    assert_eq!(
        crate::indexing::lifecycle::state::force_scan(drive.volume_id),
        Ok(RescanOutcome::DeferredUntilScanEnds),
        "the drive is being rebuilt right now, so the walk the user asked for waits for that one"
    );
    assert_eq!(
        drive.scans_started(),
        0,
        "and nothing started a second one over the top of it"
    );

    // The scan ends. Fired on this thread rather than through the spawn, so the
    // assertion is about the decision and not about timing; the release SITES that
    // reach this in production are covered by
    // `scan_completion::tests::a_rescan_queued_behind_a_scan_runs_when_its_handoff_is_done`
    // and `cover::tests::every_whole_volume_holder_runs_the_rescan_it_owes`.
    drop(scanning);
    crate::indexing::lifecycle::rescan_request::run_owed_now(drive.volume_id);
    assert_eq!(
        drive.scans_started(),
        1,
        "and the drive gets the fresh walk that was asked for"
    );
}

/// Five impatient clicks are one queued rescan, not five.
///
/// The request carries nothing but "this volume wants a full walk", so a second
/// click describes the same work — and a queue would mean a truncating rebuild per
/// click, each one blanking what the last just wrote, for as long as the user kept
/// pressing.
#[test]
fn clicking_rescan_five_times_during_a_scan_queues_one_walk() {
    let drive = ColdDrive::new("cover-rescan-clicks-dont-stack-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");

    drive.cover(&drive.path("scope"));
    drive.mark_scan_completed();

    let volume_root = drive.tree.path().to_string_lossy().to_string();
    let scanning = Claim::take(drive.volume_id, vec![volume_root], Mode::Exclusive);

    for _ in 0..5 {
        assert_eq!(
            crate::indexing::lifecycle::state::force_scan(drive.volume_id),
            Ok(RescanOutcome::DeferredUntilScanEnds),
            "every click gets the same honest answer"
        );
    }

    drop(scanning);
    crate::indexing::lifecycle::rescan_request::run_owed_now(drive.volume_id);
    assert_eq!(drive.scans_started(), 1, "one walk for five clicks");

    // And nothing is left over to fire a sixth time when the next holder leaves.
    crate::indexing::lifecycle::rescan_request::run_owed_now(drive.volume_id);
    assert_eq!(
        drive.scans_started(),
        1,
        "the request was spent by the walk that ran it"
    );
}

/// The truncate door the "Rescan now" button used to be, and where the two
/// mechanisms meet.
///
/// A drive a search walked has rows and no `scan_completed_at`, which is exactly
/// what `start_scan` reads as "a partial that never finished" and TRUNCATES. It is
/// now the phase machine's instead: the button starts the machine, which adds to
/// what the walk covered and leaves ground another walk is holding to that walk.
///
/// So the request is served immediately rather than deferred. ❌ Nothing here
/// competes with the deferred-rescan mechanism: that one exists because a
/// truncating scan can't run under a live walk, and this route never truncates, so
/// the two answer for disjoint index states. `../DETAILS.md` § "Rescan now, and
/// what it means before the first index finishes".
#[test]
fn a_rescan_during_the_phased_window_starts_the_machine_under_a_live_walk() {
    let drive = ColdDrive::new("cover-phased-rescan-under-walk-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");
    std::fs::write(drive.tree.path().join("scope/found.txt"), "x").expect("file");
    let scope = drive.path("scope");

    drive.cover(&scope);
    let walked_row = drive.path("scope/found.txt");
    assert!(drive.is_indexed(&walked_row), "precondition: the walk's rows are in");
    assert!(
        crate::indexing::lifecycle::state::force_scan(drive.volume_id).is_ok(),
        "precondition: this drive has no completed scan, so it is the machine's"
    );
    let epoch = drive.current_epoch();

    // A second walk is live on the volume, holding ground.
    let walking = Claim::take(drive.volume_id, vec![scope.clone()], Mode::Additive);

    assert_eq!(
        crate::indexing::lifecycle::state::force_scan(drive.volume_id),
        Ok(RescanOutcome::Started),
        "the machine takes the volume straight away; it has no reason to wait for a walk it composes with"
    );
    assert_eq!(
        drive.current_epoch(),
        epoch,
        "❌ and nothing truncated: every truncating (re)scan bumps the epoch before it walks"
    );
    assert!(
        drive.is_indexed(&walked_row),
        "❌ nor did the rows the search walk earned go anywhere"
    );

    drop(walking);
}

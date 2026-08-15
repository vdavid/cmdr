//! What the scoped walk does with a directory it couldn't read, over a volume it
//! reaches only through the `Volume` trait.
//!
//! Its own file because it is its own design, not a corner of the walk: on a share
//! ONE failed listing and the whole share going away arrive through the same arm,
//! and telling them apart is what these tests pin. The rule they hold the walk to
//! is that a give-up counts only once a LATER listing succeeds — the share
//! answering after this directory wouldn't — so a share that stops answering costs
//! no marks on any exit path. `network_scanner/DETAILS.md` § "A failed listing is
//! held until the share answers again" is the reasoning.
//!
//! The rest of the trait-walk suite is `network_tests.rs`, and the harness both
//! run on is `test_support.rs`.

use super::test_support::{Share, Tree, drain};

/// The tree every give-up test walks: one directory that won't list, a sibling
/// that will, and something one level under the sibling.
///
/// ⚠️ The grandchild is load-bearing, not scenery. A give-up is only recorded once
/// a LATER listing succeeds, and the walk keeps up to 64 round trips in flight —
/// so `open` and `closed` resolve in whatever order the runtime hands them back.
/// `open/deeper` is dispatched only after `open`'s result is processed, which puts
/// one guaranteed success behind both of them and makes the outcome the same every
/// run.
fn one_closed_door(t: &Tree) -> Vec<cmdr_fs::entry::FileEntry> {
    vec![
        t.dir("scope"),
        t.dir("scope/open"),
        t.file("scope/open/a.txt", 1),
        t.dir("scope/open/deeper"),
        t.file("scope/open/deeper/c.txt", 3),
        t.dir("scope/closed"),
        t.file("scope/closed/b.txt", 2),
    ]
}

/// One directory that won't list on a share that is otherwise answering: its
/// siblings are covered, and it is recorded as ground Cmdr gave up on rather than
/// handed back to every later search.
///
/// ⚠️ This is the expensive half of the bug. Left uncaused, that directory stays on
/// the coverage frontier forever, so every search scoped above it re-pays the same
/// failing listing — and over a share a failing listing is up to `LIST_TIMEOUT`
/// (120 s), eight times what the local walker pays for the same mistake. The local
/// twin measured 1,497 such directories at 101 s of a 147 s walk.
#[test]
fn a_directory_that_will_not_list_on_an_answering_share_is_given_up_on() {
    let share = Share::refusing("cover-share-one-refusal-test", one_closed_door, &["scope/closed"]);
    let scope = share.path("scope");
    let closed = share.path("scope/closed");

    let (_, outcome) = drain(share.walk(&scope).0);

    assert_eq!(outcome.roots_covered, 1, "the walk finished the root it was given");
    assert!(
        outcome.abandoned_ground,
        "and says its answer is short, which no other field hints at"
    );
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(10),
        "the walk's marks to become durable",
        || share.coverage(&scope).frontier.is_empty(),
    );

    let covered = share.coverage(&scope);
    assert_eq!(
        covered.abandoned,
        [closed],
        "the directory that wouldn't answer is reported as ground Cmdr gave up on"
    );
    assert!(
        covered.frontier.is_empty(),
        "so no later search is handed it again: {covered:?}"
    );
    assert!(
        covered.permission_denied.is_empty() && covered.declined.is_empty(),
        "and it's neither a refusal the user can act on nor a standing policy: {covered:?}"
    );
}

/// Ground the walk gave up on comes back the moment a listing succeeds over it.
///
/// The heal costs nothing extra: `mark_dirs_listed` clears the cause in the same
/// `UPDATE` that stamps the epoch, so a share that wakes up is one successful
/// listing away from being whole again — no rebuild, no separate pass, and no wait
/// for the retry backoff.
#[test]
fn ground_the_walk_gave_up_on_heals_on_a_successful_listing() {
    let (share, backend) = Share::refusing_for_now("cover-share-heal-test", one_closed_door, &["scope/closed"]);
    let scope = share.path("scope");
    let closed = share.path("scope/closed");

    drain(share.walk(&scope).0);
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(10),
        "the give-up to become durable",
        || share.coverage(&scope).abandoned == [closed.clone()],
    );

    // The share wakes up, and something walks the ground it was refused. The
    // frontier no longer offers it, so the walk is pointed at it directly — which
    // is exactly what the retry backoff buys a later search.
    backend.answer_everything();
    let (_, outcome) = drain(share.walk(&closed).0);

    assert_eq!(outcome.roots_covered, 1, "the second walk read it end to end");
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(10),
        "the healed ground to read as covered",
        || share.coverage(&scope).abandoned.is_empty(),
    );
    let covered = share.coverage(&scope);
    assert!(
        covered.abandoned.is_empty() && covered.frontier.is_empty(),
        "the whole scope answers from the index again: {covered:?}"
    );
    assert_eq!(
        share.child_ids(&closed).len(),
        1,
        "with the contents the first walk never got"
    );
}

/// The frontier node itself refusing to list is not "covered": it stays frontier,
/// and the walk says it covered nothing.
///
/// ⚠️ Without the typed root arm this reads as an ordinary skipped directory and
/// the walk reports success over ground it never saw — a search would then trust
/// an index that answers for nothing.
#[test]
fn a_frontier_root_that_will_not_list_covers_nothing() {
    let share = Share::refusing(
        "cover-share-root-refusal-test",
        |t| vec![t.dir("scope"), t.file("scope/a.txt", 1)],
        &["scope"],
    );
    let scope = share.path("scope");

    let (entries, outcome) = drain(share.walk(&scope).0);

    assert_eq!(outcome.roots_covered, 0, "nothing was covered");
    // The folder's own row is materialized before anything is listed, so it goes
    // out; its CONTENTS are what the refusal costs.
    let emitted: Vec<String> = entries.iter().map(|e| e.path.to_string_lossy().to_string()).collect();
    assert_eq!(emitted, std::slice::from_ref(&scope), "and nothing inside it was read");
    let covered = share.coverage(&scope);
    assert_eq!(
        covered.frontier,
        [scope],
        "and the scope is still waiting for a walk that can read it"
    );
    assert!(
        covered.abandoned.is_empty(),
        "❌ never given up on: one round trip that didn't land is no reason to \
         write off the folder a person asked about, and the walk has nothing else \
         to go on ({covered:?})"
    );
}

/// A run of failures stops the walk instead of churning every queued directory
/// into a silently-empty row.
///
/// The whole-volume walks carry this backstop because a disconnect that doesn't
/// map to the typed variant makes EVERY remaining listing fail instantly (~6,475
/// directories in about a second, in the reported bug). A search walk on the same
/// dead session would do exactly the same thing.
#[test]
fn a_run_of_failures_stops_the_walk_rather_than_churning() {
    let refusals: Vec<String> = (0..40).map(|i| format!("scope/d{i}")).collect();
    let refused: Vec<&str> = refusals.iter().map(String::as_str).collect();
    let share = Share::refusing(
        "cover-share-backstop-test",
        |t| {
            let mut entries = vec![t.dir("scope")];
            for i in 0..40 {
                entries.push(t.dir(&format!("scope/d{i}")));
            }
            entries
        },
        &refused,
    );

    let (_, outcome) = drain(share.walk(&share.path("scope")).0);

    assert_eq!(
        outcome.roots_covered, 0,
        "the walk stopped on the failure run rather than reporting a covered scope"
    );
    assert!(
        !outcome.cancelled,
        "nobody cancelled it — it gave up, which is different"
    );
}

/// ⚠️ **A share that goes away mid-walk condemns NOTHING.** The walk keeps every
/// directory it couldn't read on the coverage frontier, so the search after the
/// NAS wakes up walks them normally.
///
/// This is the test the whole design hangs on, and the one that makes the local
/// walker's fix unportable as it stands. Both failures wear ONE shape in the walk:
/// a share dropping its session fails listings one directory at a time, through
/// the same arm as a single directory that won't answer on a healthy share. Mark on
/// that arm as it goes and a NAS that sleeps for a minute takes every directory the
/// walk had queued out of search with it — thousands of folders, invisible until a
/// retry window that can grow to 24 hours reopens them. A hole that big is worse
/// than the re-paid listing the mark exists to save.
///
/// The share here answers exactly one listing (the scope's own) and then stops
/// being there, which is the shape of a session reset: everything already in flight
/// and everything after it fails.
#[test]
fn a_share_that_goes_away_mid_walk_gives_up_on_nothing() {
    let share = Share::going_away_after(
        "cover-share-goes-away-test",
        |t| {
            let mut entries = vec![t.dir("scope")];
            for i in 0..40 {
                entries.push(t.dir(&format!("scope/d{i}")));
                entries.push(t.file(&format!("scope/d{i}/f.txt"), 1));
            }
            entries
        },
        1,
    );
    let scope = share.path("scope");

    let (_, outcome) = drain(share.walk(&scope).0);

    assert_eq!(
        outcome.roots_covered, 0,
        "the walk gave up on the share, not on a folder"
    );
    assert!(outcome.abandoned_ground, "and admits its answer is short");

    let covered = share.coverage(&scope);
    assert!(
        covered.abandoned.is_empty(),
        "❌ not one directory is written off for a share that went away: {} of them were ({covered:?})",
        covered.abandoned.len()
    );
    assert!(
        !covered.frontier.is_empty(),
        "they all stay frontier, so the search after the share wakes up simply walks them: {covered:?}"
    );
}

/// The same, for a share that goes away with too little left to walk for the
/// backstop to ever notice.
///
/// ⚠️ This is the case that rules out "hold the give-ups and write them unless the
/// walk aborts". A small scope's queue runs dry after a handful of failures, so the
/// walk ends REPORTING SUCCESS over a share that isn't there — and a rule keyed on
/// how the walk ended would write every one of them. What the walk actually has to
/// go on is narrower and doesn't care how it ended: a give-up counts only once a
/// LATER listing succeeds, which is the share answering after that directory
/// wouldn't. Here nothing answers again, so nothing is written, on this exit path
/// or any other.
#[test]
fn a_share_that_goes_away_with_little_left_to_walk_gives_up_on_nothing() {
    let share = Share::going_away_after(
        "cover-share-goes-away-quietly-test",
        |t| {
            let mut entries = vec![t.dir("scope")];
            // Fewer than `CONSECUTIVE_FAILURE_ABORT`, so the queue drains before the
            // walk ever concludes the share is gone.
            for i in 0..8 {
                entries.push(t.dir(&format!("scope/d{i}")));
            }
            entries
        },
        1,
    );
    let scope = share.path("scope");

    let (_, outcome) = drain(share.walk(&scope).0);

    assert_eq!(
        outcome.roots_covered, 1,
        "the walk ran out of queue rather than giving up, and reports the scope covered"
    );
    assert!(
        outcome.abandoned_ground,
        "while admitting it read less than the tree holds"
    );
    let covered = share.coverage(&scope);
    assert!(
        covered.abandoned.is_empty(),
        "and wrote off none of it: nothing answered after those directories didn't, so \
         the walk never learned whose failure it was ({covered:?})"
    );
    assert_eq!(
        covered.frontier.len(),
        8,
        "every one of them is still there to walk: {covered:?}"
    );
}

/// A share's give-ups arm the retry backoff, exactly as a local drive's do.
///
/// The ladder itself (5 min → 1 h → 4 h → 24 h, persisted per volume) is pinned in
/// `writer/abandoned_retry/tests.rs`; what this pins is that a NETWORK walk reaches
/// it at all. Nothing about the mechanism is local-only, but a share whose ground
/// left the frontier with no window open would be condemned once and never retried
/// — a worse bug than the one the mark fixes, and an invisible one.
#[test]
fn a_shares_give_ups_arm_the_retry_backoff() {
    let share = Share::refusing("cover-share-retry-armed-test", one_closed_door, &["scope/closed"]);
    let scope = share.path("scope");

    drain(share.walk(&scope).0);
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(10),
        "the give-up to become durable",
        || share.coverage(&scope).abandoned == [share.path("scope/closed")],
    );

    assert!(
        share.retry_window_is_open(),
        "the share's own index carries an open retry window, so the ground it gave up \
         on is reopened on the backoff rather than written off for good"
    );
}

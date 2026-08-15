//! The cover driver over a volume the index can only reach through the `Volume`
//! trait: a share, a phone, and whatever backend comes next.
//!
//! The local halves live in `tests.rs` (an index over a real temp tree) and
//! `cold_drive_tests.rs` (a drive with no index), because the guarded walker reads
//! the disk. Nothing here touches a disk at all: the ground is an `InMemoryVolume`,
//! which is exactly the shape a future backend arrives in. The share harness and
//! the instrumented backends are in `test_support.rs`.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use cmdr_fs::volume::InMemoryVolume;

use super::test_support::{SameNameSiblings, Share, Tree, drain};
use super::*;
use crate::indexing::lifecycle::rescan_request::RescanOutcome;

// ── The scoped walk ──────────────────────────────────────────────────

/// A walk over a share covers the folder it was pointed at, hands every entry to
/// its consumer, and claims nothing else on the volume.
///
/// The second half is what "scoped" means, and it's the whole milestone: the only
/// walk `network_scanner` had was the whole volume, so a search of one folder on a
/// 10 TB NAS would have walked the NAS.
#[test]
fn a_walk_over_a_share_covers_the_folder_it_was_pointed_at() {
    let share = Share::new("cover-share-scoped-test", |t| {
        vec![
            t.dir("scope"),
            t.dir("elsewhere"),
            t.file("scope/one.txt", 4),
            t.dir("scope/inner"),
            t.file("scope/inner/two.txt", 2),
            t.file("elsewhere/other.txt", 9),
        ]
    });
    let scope = share.path("scope");

    let cold = share.coverage(&scope);
    assert_eq!(cold.frontier, vec![scope.clone()], "nothing is covered yet");

    let (entries, outcome) = share.cover(&scope);

    assert!(!outcome.cancelled, "the walk ran to the end");
    assert_eq!(outcome.roots_covered, 1);
    assert_eq!(
        outcome.entries_found, 4,
        "scope/ itself, one.txt, inner/, inner/two.txt"
    );
    assert_eq!(outcome.dirs_found, 2, "scope/ and inner/");

    let mut emitted: Vec<String> = entries.iter().map(|e| e.path.to_string_lossy().to_string()).collect();
    emitted.sort();
    assert_eq!(
        emitted,
        vec![
            scope.clone(),
            share.path("scope/inner"),
            share.path("scope/inner/two.txt"),
            share.path("scope/one.txt")
        ],
        "every entry the walk wrote reached the consumer, the frontier root included: \
         its row is the walk's to report, and a search scoped to it answers with it"
    );
    let one = entries
        .iter()
        .find(|e| e.path.ends_with("one.txt"))
        .expect("one.txt emitted");
    assert_eq!(one.logical_size, Some(4), "with the size a result row shows");

    assert_eq!(
        share.coverage(&share.path("")).frontier,
        vec![share.path("")],
        "and the rest of the share is untouched: nothing may claim coverage it didn't earn"
    );
}

// ── Cancellation ─────────────────────────────────────────────────────

/// A walk over a share stopped partway KEEPS every directory it read.
///
/// This is convergence, on the far side of a network round trip, and it's the
/// property the whole coverage concept rests on: eight minutes of walking a NAS
/// that someone then cancels has to leave the frontier genuinely smaller, or no
/// amount of searching ever shrinks it. ⚠️ It is the OPPOSITE of what the
/// whole-volume scan on the same transport does — that one discards its partial,
/// because a half-built index of a share is not an index of the share.
#[test]
fn a_cancelled_walk_over_a_share_keeps_the_ground_it_covered() {
    let (share, volume) = Share::instrumented(
        "cover-share-cancel-test",
        |t| {
            vec![
                t.dir("scope"),
                t.dir("scope/inner"),
                t.file("scope/inner/deep.txt", 3),
                t.file("scope/top.txt", 1),
            ]
        },
        // The scope's OWN listing blocks, so exactly one round trip is in flight
        // when the cancel lands and the walk stops in the same place every run.
        Some("scope"),
    );
    let scope = share.path("scope");

    let (walk, cancel) = share.walk(&scope);
    volume.wait_for_the_gate();
    cancel.cancel();
    volume.release_the_gate();

    let (entries, outcome) = drain(walk);
    assert!(outcome.cancelled, "it was stopped, and says so");
    assert!(
        outcome.entries_found >= 2,
        "the totals carry what it read, not zero (got {outcome:?})"
    );
    assert!(!entries.is_empty(), "and the consumer got them");

    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(10),
        "the covered half of the scope to become durable",
        || share.coverage(&scope).frontier == [share.path("scope/inner")],
    );
    assert_eq!(
        share.coverage(&scope).frontier,
        [share.path("scope/inner")],
        "the directory the walk read is covered, and only what it never reached is left"
    );
}

/// The backend's scan session is opened once per walk and closed on every
/// outcome, cancel included.
///
/// Over SMB that session is a pool of extra TCP connections. An unpaired open
/// leaves them standing for the life of the app, and a walk somebody cancels is
/// exactly the case where nothing else would ever close them.
#[test]
fn the_scan_session_is_paired_on_the_completed_and_the_cancelled_walk() {
    let (share, volume) = Share::instrumented(
        "cover-share-session-test",
        |t| {
            vec![
                t.dir("first"),
                t.file("first/a.txt", 1),
                t.dir("second"),
                t.dir("second/inner"),
                t.file("second/inner/b.txt", 1),
            ]
        },
        Some("second"),
    );

    share.cover(&share.path("first"));
    assert_eq!(
        volume.sessions(),
        (1, 1),
        "a completed walk opens one session and closes it"
    );

    let (walk, cancel) = share.walk(&share.path("second"));
    volume.wait_for_the_gate();
    cancel.cancel();
    volume.release_the_gate();
    let (_, outcome) = drain(walk);

    assert!(outcome.cancelled);
    assert_eq!(
        volume.sessions(),
        (2, 2),
        "and so does a cancelled one: the pool never outlives the walk that opened it"
    );
}

/// A walk that gets NONE of the ground it asked for runs nothing at all: no
/// session, no listing, and an answer on the spot.
///
/// This is the promptness the search dialog hangs on. A walk with no ground used
/// to go through the whole of `walk_frontier` anyway, and its tail commits the
/// writer — which parks behind everything already queued. Behind a drive's first
/// index that measured 4.5-5.8 s in the app on a warm boot disk (and 35 s on a
/// cold one), all of it spent committing nothing, and the search that asked said
/// "0 matches" for the whole wait instead of naming the walk it's queued behind.
///
/// The session counter is what makes it checkable without timing anything: the
/// backend is opened at the top of `walk_frontier` and the commit is at the
/// bottom, so a walk that never opened one never reached either.
#[test]
fn a_walk_that_gets_no_ground_opens_no_session_and_answers_on_the_spot() {
    let (share, volume) = Share::instrumented(
        "cover-share-no-ground-test",
        |t| vec![t.dir("scope"), t.file("scope/one.txt", 4)],
        None,
    );
    let scope = share.path("scope");

    // Somebody else's live walk, stood in for by its claim — the same stand-in
    // `a_walk_leaves_ground_another_walk_is_covering_to_it` uses, and for the same
    // reason: a real first walk over a fixture this small can finish before the
    // second one starts.
    let held = Claim::take("cover-share-no-ground-test", vec![scope.clone()]);

    let (walk, _cancel) = share.walk(&scope);
    assert_eq!(
        walk.covered_by_another_walk(),
        std::slice::from_ref(&scope),
        "every root it asked for is the other walk's"
    );
    let (entries, outcome) = drain(walk);

    assert!(entries.is_empty(), "nothing to emit: it read nothing");
    assert_eq!(outcome, CoverOutcome::nothing(false), "and nothing to report");
    assert_eq!(
        volume.sessions(),
        (0, 0),
        "the backend is never opened for a walk with nowhere to go"
    );

    drop(held);
}

// ── Pacing ───────────────────────────────────────────────────────────

/// The walk overlaps its round trips, and never past the pacer's budget.
///
/// Directory listing over a share is latency-bound, so a serial scoped walk would
/// be an order of magnitude slower than it needs to be — and the ceiling is the
/// same per-volume budget the background scan yields with, so a search walking a
/// share the user is also browsing drops to one listing in flight instead of
/// burying their navigation behind 64.
#[test]
fn the_walk_overlaps_its_listings_within_the_pacer_budget() {
    // More subdirectories than the budget, so "up to 64 in flight" is a claim the
    // fixture can actually falsify.
    let subdirs = crate::indexing::network_scanner::scan_pace::FULL_LISTING_BUDGET as u64 + 6;
    let (share, volume) = Share::instrumented(
        "cover-share-pacing-test",
        |t| {
            let mut entries = vec![t.dir("scope")];
            for i in 0..subdirs {
                entries.push(t.dir(&format!("scope/d{i}")));
                entries.push(t.file(&format!("scope/d{i}/f.txt"), 1));
            }
            entries
        },
        None,
    );

    let (_, outcome) = share.cover(&share.path("scope"));
    assert_eq!(
        outcome.dirs_found,
        subdirs + 1,
        "every subdirectory was walked, plus the frontier root's own row"
    );

    let max_in_flight = volume.max_in_flight.load(Ordering::SeqCst);
    assert!(
        max_in_flight > 1,
        "the walk overlaps its listings rather than going one at a time (max was {max_in_flight})"
    );
    assert!(
        max_in_flight <= crate::indexing::network_scanner::scan_pace::FULL_LISTING_BUDGET as u64,
        "and never past the budget the pacer hands out (max was {max_in_flight})"
    );
}

// ── Ground somebody already touched ──────────────────────────────────

/// A walk over a frontier node the index already holds rows under keeps them: the
/// pre-existing rows keep their ids, the new siblings arrive, and nothing is
/// deleted.
///
/// The shape takes one earlier walk to produce — cover a deep folder, and the
/// ancestor it had to materialize is a frontier node with a covered child under
/// it. The local walker refuses this case and hands it to the serial reconcile;
/// over the trait the walk simply takes it, because comparing a directory's names
/// against the index costs nothing next to the round trip that listed them.
#[test]
fn a_walk_over_ground_an_earlier_walk_touched_keeps_its_rows() {
    let share = Share::new("cover-share-existing-rows-test", |t| {
        vec![
            t.dir("F"),
            t.dir("F/G"),
            t.file("F/G/kept.txt", 4),
            t.file("F/new.txt", 3),
        ]
    });

    // The first walk covers G and materializes F on the way, without listing it.
    share.cover(&share.path("F/G"));
    let g_rows = share.child_ids(&share.path("F/G"));
    assert_eq!(g_rows.len(), 1, "precondition: G holds kept.txt");
    assert_eq!(
        share.coverage(&share.path("F")).frontier,
        [share.path("F")],
        "precondition: F itself is a frontier node"
    );

    let (entries, outcome) = share.cover(&share.path("F"));

    assert_eq!(outcome.roots_covered, 1);
    assert_eq!(
        share.child_ids(&share.path("F/G")),
        g_rows,
        "the rows this walk did not write keep their ids"
    );
    assert_eq!(share.child_ids(&share.path("F")).len(), 2, "G, plus the new sibling");
    let emitted: Vec<String> = entries.iter().map(|e| e.path.to_string_lossy().to_string()).collect();
    assert_eq!(
        emitted,
        [share.path("F/new.txt")],
        "and only the row it actually wrote is emitted, so a live search sees no duplicates"
    );
}

/// The truncate door on this half of the walk, and the same refusal:
/// `start_volume_scan`'s single-flight guard reads `mgr.scanning`, which a
/// search-driven walk never sets — it holds a claim.
///
/// A NAS "Rescan now" landing mid-walk would `TruncateData` + `BumpCurrentEpoch`
/// under a walk that is still writing rows over the wire, which is the slowest
/// walk we have and so the widest window. The local half carries the epoch
/// assertion (`cold_drive_tests`); what this pins is that the trait scanner asks
/// the same question.
#[test]
fn a_truncating_rescan_of_a_share_refuses_while_a_cover_walk_is_live() {
    let volume_id = "cover-share-truncate-guard-test";
    let share = Share::new(volume_id, |t| vec![t.dir("scope"), t.file("scope/found.txt", 2)]);
    share.cover(&share.path("scope"));
    let rows = share.child_ids(&share.path("scope"));
    assert_eq!(rows.len(), 1, "precondition: the walk's rows are in");

    let walking = Claim::take(volume_id, vec![share.path("scope")]);
    assert_eq!(
        crate::indexing::lifecycle::state::force_scan(volume_id),
        Ok(RescanOutcome::Deferred),
        "a rescan waits for the walk holding ground on the share, and says so in a variant"
    );
    assert_eq!(
        share.child_ids(&share.path("scope")),
        rows,
        "so the walk's rows keep the ids it wrote them under"
    );

    drop(walking);
    assert_eq!(
        crate::indexing::lifecycle::state::force_scan(volume_id),
        Ok(RescanOutcome::Started),
        "and once the walk ends the rescan runs"
    );
}

// ── Same-name siblings (MTP) ─────────────────────────────────────────

/// Two children with one name: the walk keeps the first and says so, instead of
/// allocating a second id whose rows vanish.
///
/// The index can hold one row per `(parent, folded name)`, and `insert_entries_v2_batch`
/// is `INSERT OR IGNORE` — so without the per-directory name check the second
/// `dup` would get an id, be queued as a directory of its own, have its children
/// written under that id, and then lose the row that id belonged to. Everything
/// below it would be orphaned: rows in the database that no path resolves to,
/// invisible to search and counted in nobody's size.
#[test]
fn a_same_name_sibling_keeps_the_first_row_rather_than_orphaning_a_subtree() {
    let share = Share::with_volume("cover-share-mtp-siblings-test", |root| {
        let tree = Tree(root.to_string());
        let inner = InMemoryVolume::with_entries(
            "Phone",
            vec![
                tree.dir("scope"),
                tree.dir("scope/dup"),
                tree.file("scope/dup/child.txt", 7),
            ],
        )
        .with_root(root);
        Arc::new(SameNameSiblings {
            inner,
            doubled: PathBuf::from(tree.path("scope")),
        })
    });
    let scope = share.path("scope");

    let (entries, outcome) = share.cover(&scope);

    assert_eq!(
        outcome.entries_found, 3,
        "the frontier root, dup/ once, and its child once"
    );
    let mut emitted: Vec<String> = entries.iter().map(|e| e.path.to_string_lossy().to_string()).collect();
    emitted.sort();
    assert_eq!(
        emitted,
        [
            scope.clone(),
            share.path("scope/dup"),
            share.path("scope/dup/child.txt")
        ]
    );
    assert_eq!(
        share.child_ids(&scope).len(),
        1,
        "one row for the name, and no second id pointing at nothing"
    );
    assert_eq!(
        share.child_ids(&share.path("scope/dup")).len(),
        1,
        "and the subtree below it is attributed to the row that survived"
    );
}

// ── NAS system directories ───────────────────────────────────────────

/// A search never walks into a NAS snapshot directory, and the frontier stops
/// offering it after the first walk that sees it.
///
/// ⚠️ This is the stall the network scanner exists to avoid, arriving by a new
/// route. `@Recently-Snapshot` and friends are hardlinked, huge, and re-walking
/// them costs a full traversal PER SNAPSHOT over serialized SMB — one reported
/// 44 TB on a 10 TB volume. Both whole-volume walks index the directory's own row
/// and refuse its subtree, which leaves it `listed_epoch = 0`, which is exactly
/// what the descent rule calls FRONTIER. So without this, every search of a NAS
/// hands the walk the one tree nobody may walk.
#[test]
fn a_nas_system_dir_is_indexed_but_never_walked_into() {
    let share = Share::new("cover-share-nas-dirs-test", |t| {
        vec![
            t.dir("scope"),
            t.file("scope/real.txt", 1),
            t.dir("scope/@Recently-Snapshot"),
            t.dir("scope/@Recently-Snapshot/2026-08-01"),
            t.file("scope/@Recently-Snapshot/2026-08-01/huge.bin", 999),
        ]
    });
    let scope = share.path("scope");
    let snapshot = share.path("scope/@Recently-Snapshot");

    let (entries, outcome) = share.cover(&scope);

    assert_eq!(
        outcome.entries_found, 3,
        "the frontier root, real.txt, and the snapshot dir's own row"
    );
    let emitted: Vec<String> = entries.iter().map(|e| e.path.to_string_lossy().to_string()).collect();
    assert!(
        !emitted.iter().any(|p| p.contains("huge.bin")),
        "nothing inside the snapshot tree was walked: {emitted:?}"
    );
    assert_eq!(
        share.child_ids(&snapshot),
        Vec::<i64>::new(),
        "its subtree is honestly unknown rather than half-indexed"
    );

    let covered = share.coverage(&scope);
    assert!(
        covered.frontier.is_empty(),
        "and the frontier does NOT hand the snapshot tree back to the next search: {covered:?}"
    );
    assert_eq!(
        covered.declined,
        [snapshot],
        "it's reported as ground Cmdr won't read, which is what it is"
    );
    assert!(
        covered.permission_denied.is_empty(),
        "and never as a permission the user could grant: nobody refused us, we declined"
    );
}

/// The same, for a frontier that names the snapshot directory itself — which is
/// what an index built before this rule looks like, and what a walk that stopped
/// just above one leaves behind.
#[test]
fn a_frontier_rooted_at_a_nas_system_dir_is_refused_rather_than_walked() {
    let share = Share::new("cover-share-nas-root-test", |t| {
        vec![
            t.dir("@eaDir"),
            t.dir("@eaDir/inner"),
            t.file("@eaDir/inner/thumb.jpg", 42),
        ]
    });
    let snapshot = share.path("@eaDir");

    let (entries, outcome) = share.cover(&snapshot);

    // The directory itself is a row like any other — an indexed NAS holds one for
    // it too, and a search that matches its name finds it — so it reaches the
    // consumer. What must not is anything INSIDE it.
    let emitted: Vec<String> = entries.iter().map(|e| e.path.to_string_lossy().to_string()).collect();
    assert_eq!(
        emitted,
        std::slice::from_ref(&snapshot),
        "not one round trip's worth of it was walked"
    );
    assert_eq!(
        outcome.entries_found, 1,
        "the snapshot directory's own row, and nothing under it"
    );
    assert!(
        share.coverage(&snapshot).frontier.is_empty(),
        "and it isn't handed back to the next search either"
    );
}

// ── When a listing fails ─────────────────────────────────────────────

/// One directory that won't list doesn't stop the walk: its siblings are covered,
/// and it stays frontier for a later search to try again.
///
/// Honest-stale, never false-complete — the same rule the local walker follows for
/// a directory it can't read.
#[test]
fn an_unlistable_directory_is_skipped_and_the_rest_is_covered() {
    let share = Share::refusing(
        "cover-share-one-refusal-test",
        |t| {
            vec![
                t.dir("scope"),
                t.dir("scope/open"),
                t.file("scope/open/a.txt", 1),
                t.dir("scope/closed"),
                t.file("scope/closed/b.txt", 2),
            ]
        },
        &["scope/closed"],
    );
    let scope = share.path("scope");

    let (_, outcome) = drain(share.walk(&scope).0);

    assert_eq!(outcome.roots_covered, 1, "the walk finished the root it was given");
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(10),
        "the readable half of the scope to read as covered",
        || share.coverage(&scope).frontier == [share.path("scope/closed")],
    );
    assert_eq!(
        share.coverage(&scope).frontier,
        [share.path("scope/closed")],
        "and only the directory it couldn't read is left to try again"
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
    assert_eq!(
        share.coverage(&scope).frontier,
        [scope],
        "and the scope is still waiting for a walk that can read it"
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

// ── Batching ─────────────────────────────────────────────────────────

/// A big directory reaches its consumer in several bounded batches, not one
/// giant one and not one crossing per entry.
///
/// Decision 3's shape: one channel crossing per batch is what keeps the walk in
/// `indexing/` and the matching in `search/` without a per-entry cost, and the
/// bound is what stops a queue growing to the size of the subtree.
#[test]
fn a_big_directory_arrives_in_bounded_batches() {
    let count = 2_500;
    let share = Share::new("cover-share-batching-test", |t| {
        let mut entries = vec![t.dir("scope")];
        for i in 0..count {
            entries.push(t.file(&format!("scope/f{i:05}.txt"), 1));
        }
        entries
    });

    let (walk, _cancel) = share.walk(&share.path("scope"));
    let mut sizes = Vec::new();
    while let Some(batch) = walk.next_batch() {
        sizes.push(batch.len());
    }
    let outcome = walk.finish();

    // The frontier root rides with them, in a batch of its own before the rest.
    assert_eq!(outcome.entries_found, count as u64 + 1);
    assert_eq!(
        sizes.iter().sum::<usize>(),
        count + 1,
        "every entry reached the consumer"
    );
    assert!(sizes.len() > 1, "in more than one batch (got {sizes:?})");
    assert!(
        sizes.iter().all(|size| *size <= 2000),
        "none of them unbounded (got {sizes:?})"
    );
    assert!(
        sizes.iter().any(|size| *size > 1),
        "and one crossing per BATCH, not per entry (got {sizes:?})"
    );
}

// ── A phone ──────────────────────────────────────────────────────────

/// The same walk over an MTP-rooted volume, whose paths are `mtp://…` and whose
/// files exist only over PTP.
///
/// Worth its own test rather than trusting the share's: a phone has no mount to
/// probe and no path any `statfs` can answer for, so it takes a different arm of
/// the classifier, and its index is rooted at a URL rather than a directory. If
/// the path space were wrong anywhere along the way, the walk would write rows
/// nothing could ever resolve.
#[test]
fn a_walk_over_a_phone_covers_the_folder_it_was_pointed_at() {
    let volume_id = "mtp-COVERTEST:1";
    let root = "mtp://mtp-COVERTEST/1";
    let serialized = crate::indexing::handle::test_lock();
    let data = tempfile::tempdir().expect("index data dir");

    let tree = Tree(root.to_string());
    let volumes = crate::indexing::host::volumes::FakeVolumeProvider::shared();
    volumes.register(
        volume_id,
        Arc::new(
            InMemoryVolume::with_entries(
                "Phone",
                vec![
                    tree.dir("DCIM"),
                    tree.dir("DCIM/Camera"),
                    tree.file("DCIM/Camera/IMG_0001.jpg", 2_048),
                    tree.dir("Music"),
                ],
            )
            .with_root(root),
        ) as Arc<dyn Volume>,
    );

    let (index, _installed) = crate::indexing::handle::Index::builder()
        .data_dir(data.path())
        .volumes(Arc::clone(&volumes) as Arc<_>)
        .events(Arc::new(crate::indexing::events::RecordingSink::new()) as Arc<dyn crate::indexing::events::EventSink>)
        .install_for_test();

    let scope = tree.path("DCIM");
    let coverage = |path: &str| {
        index
            .coverage(volume_id, path, CoverageDimension::Listing)
            .expect("the phone answers for its own coverage")
    };
    assert_eq!(
        coverage(&scope).frontier,
        std::slice::from_ref(&scope),
        "nothing is covered yet"
    );

    let (entries, outcome) = drain(
        index
            .cover(
                volume_id,
                vec![scope.clone()],
                CoverageDimension::Listing,
                CancellationToken::new(),
            )
            .expect("a phone is walkable"),
    );

    assert!(!outcome.cancelled);
    assert_eq!(outcome.entries_found, 3, "DCIM/ itself, Camera/, and the photo in it");
    let mut emitted: Vec<String> = entries.iter().map(|e| e.path.to_string_lossy().to_string()).collect();
    emitted.sort();
    assert_eq!(
        emitted,
        [
            scope.clone(),
            tree.path("DCIM/Camera"),
            tree.path("DCIM/Camera/IMG_0001.jpg")
        ]
    );

    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(10),
        "the walked folder to read as covered",
        || coverage(&scope).frontier.is_empty(),
    );
    assert_eq!(
        coverage(root).frontier,
        [root.to_string()],
        "and the rest of the phone is untouched"
    );

    let _ = index.forget_volume(volume_id);
    drop(serialized);
}

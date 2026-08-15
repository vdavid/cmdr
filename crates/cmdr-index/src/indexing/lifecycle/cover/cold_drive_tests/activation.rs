//! The index a walk stands up on a drive that has none: what the bootstrap
//! claims, what a second walk reuses, what a later enable does to it, and the
//! coverage stamp that decides whether any of it counts.

use super::*;

/// A drive nobody ever indexed, driven through the public handle: the walk
/// stands the whole index up (database, epoch, writer, the chain down to the
/// scope), covers exactly the folder it was pointed at, and claims NOTHING
/// else.
///
/// The second half is the load-bearing one. A bootstrap that claimed anything
/// beyond what it read would make the very next search skip ground nobody has
/// walked, and a search that quietly omits a folder is the bug this whole effort
/// exists to remove. So the volume root — the ancestor the bootstrap had to
/// materialize to reach the scope — must still read as uncovered afterwards.
#[test]
fn a_cold_volume_bootstraps_and_claims_only_what_it_walked() {
    let drive = ColdDrive::new("cover-cold-bootstrap-test");
    std::fs::create_dir_all(drive.tree.path().join("scope/inner")).expect("dirs");
    std::fs::write(drive.tree.path().join("scope/inner/found.txt"), "x").expect("file");
    std::fs::create_dir_all(drive.tree.path().join("elsewhere")).expect("dirs");
    let scope = drive.path("scope");
    let volume_root = drive.path("");

    let cold = drive.coverage(&scope);
    assert_eq!(cold.frontier, vec![scope.clone()], "nothing is covered yet");
    assert_eq!(cold.token, crate::indexing::read::coverage::CoverageToken::UNINDEXED);

    let outcome = drive.cover(&scope);
    assert!(!outcome.cancelled);
    assert_eq!(outcome.roots_covered, 1, "the scope was covered");
    assert_eq!(outcome.entries_found, 3, "scope/ itself, inner/, and inner/found.txt");

    let whole_volume = drive.coverage(&volume_root);
    assert_eq!(
        whole_volume.frontier,
        vec![volume_root],
        "the volume root was materialized, not listed: nothing may claim coverage it didn't earn"
    );
    assert_ne!(
        whole_volume.token,
        crate::indexing::read::coverage::CoverageToken::UNINDEXED,
        "and the volume now has an index to answer from"
    );
}

/// A walk over a drive whose index is left over from an earlier session reads it
/// as Stale, never Fresh.
///
/// Fresh-on-launch is what a journal REPLAY earns, and a writer-only start
/// doesn't replay: nothing has been watching this volume, so its rows are
/// stale-but-visible. Claiming Fresh would make the badge say "authoritative"
/// over an index nobody has verified since the app was last open.
#[test]
fn a_walk_on_a_left_over_index_reads_it_as_stale() {
    let drive = ColdDrive::new("cover-cold-leftover-test");
    {
        // A local index a previous session completed, with nothing running it now.
        drop(IndexStore::open(&drive.db_path()).expect("open store"));
        let conn = IndexStore::open_write_connection(&drive.db_path()).expect("write connection");
        IndexStore::update_meta(&conn, "scan_completed_at", "1700000000").expect("stamp a completed scan");
    }

    // Started as the JOURNALED kind on purpose: the boot disk is the only kind
    // that can load Fresh at all, so it's the only one where this can go wrong.
    crate::indexing::lifecycle::state::start_indexing_for(
        drive.volume_id,
        drive.tree.path().to_path_buf(),
        IndexVolumeKind::Local,
        true,
        crate::indexing::lifecycle::state::Activation::WriterOnly,
    )
    .expect("stand the index up for a walk");

    assert_eq!(
        crate::indexing::lifecycle::state::get_freshness(drive.volume_id),
        Some(crate::indexing::lifecycle::freshness::Freshness::Stale),
        "a walk replays no journal, so it verifies nothing and may not claim Fresh"
    );
}

/// The second walk on a bootstrapped drive reuses the writer the first one stood
/// up, and the coverage the first one earned is still there.
///
/// One writer per database is the invariant: a second would allocate ids from its
/// own counter and inflate `dir_stats`, and a second bootstrap that re-prepared
/// the database would throw the first walk's ground away.
#[test]
fn a_second_walk_reuses_the_index_the_first_one_stood_up() {
    let drive = ColdDrive::new("cover-cold-second-walk-test");
    for name in ["first", "second"] {
        std::fs::create_dir_all(drive.tree.path().join(name)).expect("dirs");
        std::fs::write(drive.tree.path().join(name).join("f.txt"), "x").expect("file");
    }

    drive.cover(&drive.path("first"));
    drive.cover(&drive.path("second"));

    assert!(
        drive.coverage(&drive.path("first")).frontier.is_empty(),
        "the first walk's ground survived the second walk"
    );
}

/// Turning indexing on for a drive a search already walked indexes the whole
/// drive, instead of no-opping against the index the walk left — and it does it
/// WITHOUT throwing that index away.
///
/// A walk registers an instance with no scan and no watcher behind it, so a bare
/// "this volume is already active" would swallow the request for exactly those.
/// A first scan someone stopped leaves the same shape, and had the same problem.
///
/// The second half is the truncate door. This is the path the per-drive "Turn on
/// indexing for this drive" button takes, and the one the FDA-deny start takes on
/// launch (`start_indexing_after_fda_decision` → `start_volume`), and it used to
/// reach `force_scan` → `start_scan` → `TruncateData` — on precisely the volumes
/// that have covered ground worth keeping. ❌ Don't re-key `awaits_its_first_scan`
/// to close it: it's shared, and it exists for two shapes that both have rows.
/// The routing lives one level down, in `cover_or_scan`.
#[tokio::test(flavor = "multi_thread")]
#[allow(
    clippy::await_holding_lock,
    reason = "the fixture holds the process-wide seams for the whole test; holding it across the await IS the point"
)]
async fn turning_indexing_on_after_a_walk_covers_the_drive_without_truncating_it() {
    let drive = ColdDrive::new("cover-cold-then-enable-test");
    std::fs::create_dir_all(drive.tree.path().join("walked")).expect("dirs");
    std::fs::create_dir_all(drive.tree.path().join("never-walked")).expect("dirs");
    let volume_root = drive.path("");

    drive.cover(&drive.path("walked"));
    assert!(
        !drive.coverage(&volume_root).frontier.is_empty(),
        "precondition: one walked folder leaves the rest of the drive uncovered"
    );
    let epoch_the_walk_wrote_against = drive.current_epoch();

    drive
        .index
        .start_volume(drive.volume_id)
        .await
        .expect("the drive starts indexing");

    assert_eq!(
        drive.current_epoch(),
        epoch_the_walk_wrote_against,
        "❌ the enable must not truncate what the search walked: every truncating (re)scan bumps \
         the epoch before it walks, so a bump here IS the door being open"
    );

    // Waited on the DURABLE completion marker, not on the coverage answer: a walk
    // marks its directories listed well before `scan_completed_at` reaches the
    // database, so "the drive reads as covered" would let the second enable below
    // find a scan that hasn't finished recording itself yet (it did, on Linux).
    cmdr_fs::testing::wait_until_async(
        std::time::Duration::from_secs(20),
        "the full scan to finish and record itself",
        || drive.index.volume_status(drive.volume_id).scan_completed_at.is_some(),
    )
    .await;
    assert!(
        drive.coverage(&volume_root).frontier.is_empty(),
        "and the whole drive is covered now"
    );
    assert_eq!(drive.scans_started(), 1, "exactly one scan, not one per call");

    // And the other side of the same gate: a drive that HAS been indexed must not
    // be rescanned by an enable. On a real drive that's a full re-walk of
    // everything — minutes on a NAS — off one stray click.
    drive
        .index
        .start_volume(drive.volume_id)
        .await
        .expect("a second enable is a no-op");
    assert_eq!(drive.scans_started(), 1, "an indexed drive is left alone");
}

/// An index whose rows predate this build's exclusion policy is DROPPED before
/// the next walk, not walked on top of (`docs/specs/unindexed-search-plan.md`
/// Decision 17).
///
/// A stamp mismatch means nothing in the index counts as covered, so every
/// search hands the whole scope back to the walk. On a scanned drive the next
/// full scan truncates and re-stamps; on a walk-built one no scan is coming, and
/// the walk can't re-stamp a database that already holds rows — so without the
/// eviction that drive re-walks its whole scope on every search, forever, each
/// root landing on the slow repair path. Evicting costs one walk and gets
/// convergence back.
#[test]
fn an_index_that_predates_the_exclusion_policy_is_dropped_before_the_next_walk() {
    let drive = ColdDrive::new("cover-stale-policy-test");
    std::fs::create_dir_all(drive.tree.path().join("scope/inner")).expect("dirs");
    std::fs::write(drive.tree.path().join("scope/inner/found.txt"), "x").expect("file");
    let scope = drive.path("scope");

    drive.cover(&scope);
    assert!(
        drive.is_indexed(&drive.path("scope/inner/found.txt")),
        "the walk landed"
    );

    // The drive stops being indexed (the user turned it off, or the app simply
    // restarted and nothing registered a drive nobody indexes), and a release
    // edits the exclusion lists behind its back.
    drive.index.disable_volume(drive.volume_id).expect("disable");
    {
        let conn = IndexStore::open_write_connection(&drive.db_path()).expect("write conn");
        IndexStore::update_meta(&conn, crate::indexing::store::EXCLUSION_POLICY_KEY, "0000000000000000")
            .expect("stamp a policy this build doesn't apply");
    }

    // The next search's walk converges again, which it can only do on a database
    // it was allowed to stamp — an empty one.
    let outcome = drive.cover(&scope);
    assert!(!outcome.cancelled);
    assert!(
        outcome.entries_found >= 2,
        "the walk rebuilt what the dropped index held, {} entries",
        outcome.entries_found
    );
    let conn = IndexStore::open_read_connection(&drive.db_path()).expect("read conn");
    assert!(
        !crate::indexing::scanner::index_predates_exclusion_policy(&conn),
        "the rebuilt index carries this build's policy, so its coverage counts"
    );
}

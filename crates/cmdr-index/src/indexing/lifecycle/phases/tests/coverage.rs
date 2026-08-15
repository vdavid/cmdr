//! Whether a phase actually covers ground: the frontier shrinking, the stitch
//! that lets the next phase start where the last one stopped, the exclusion stamp
//! every coverage answer depends on, and the refusal that keeps a truncating scan
//! out while the machine has work.

use super::*;

/// **The one that makes every other test here meaningful.** A phased start never
/// goes through `start_scan`, so nothing else writes the exclusion-policy stamp —
/// and without it `index_predates_exclusion_policy` answers yes, every coverage
/// query short-circuits to "the whole scope is frontier", the frontier never
/// shrinks, and each later root lands on the serial repair. The product would look
/// like it was working while never converging.
#[test]
fn a_fresh_phased_volume_s_frontier_shrinks_after_one_walk() {
    let drive = Drive::new(
        "phased-converges",
        |root| {
            std::fs::create_dir_all(root.join("one/inside")).expect("dirs");
            std::fs::create_dir_all(root.join("two")).expect("dirs");
            std::fs::write(root.join("one/file.txt"), "x").expect("file");
        },
        &[],
    );

    drive.start();
    drive.wait_for_the_machine();

    assert!(
        drive.frontier(&drive.path("")).is_empty(),
        "the whole drive reads as covered once the machine has been over it"
    );
    assert!(
        drive.meta("scan_completed_at").is_some(),
        "and an empty frontier is what completion means"
    );
    assert_eq!(
        drive.scans_started(),
        1,
        "the machine announces one run; ❌ no truncating full scan ever ran"
    );
}

// ── The stitch ───────────────────────────────────────────────────────

/// The finding that broke the first draft of the design: a cover walk marks only
/// the directories it READS, so covering one child leaves the parent's frontier
/// saying "walk the parent whole" — the later phase would re-walk everything the
/// earlier one covered, and hit the serial repair path doing it.
///
/// The stitch is what makes an ancestor scope's frontier shrink, and every root
/// it leaves has to be virgin, or the parallel walker refuses it.
#[test]
fn frontier_excludes_covered_ground_after_a_stitch() {
    let t = Tree::new();
    t.make(
        &["covered/inside", "untouched/inside"],
        &["covered/one.txt", "loose.txt"],
    );
    let root = t.root().to_string_lossy().to_string();

    // A priority phase covers one child of the tree root.
    t.seed_chain(&t.tree.path().join("covered"));
    t.cover(&t.path("covered"));

    // The later phase stitches the tree root before asking what is left.
    stitch::directory(&t.space, &t.writer, t.root());

    assert_eq!(
        t.frontier(&root),
        vec![t.path("untouched")],
        "the covered child is gone from the frontier and the untouched one is offered whole"
    );
    for frontier_root in t.frontier(&root) {
        let conn = IndexStore::open_read_connection(&t.db_path).expect("read connection");
        let id = crate::indexing::store::resolve_path(&conn, &frontier_root)
            .expect("resolve")
            .expect("a frontier root has a row");
        assert_eq!(
            IndexStore::count_children_capped(id, &conn, 1).expect("count"),
            0,
            "{frontier_root} must be virgin, or the parallel walker refuses it and the serial repair takes over"
        );
    }
}

/// `listed_children_on` serves a directory's rows as its FULL contents the moment
/// its `listed_epoch` is non-zero, and the MCP `list_dir` tool reads exactly that.
/// So a stitch that upserted only subdirectories would tell a user-visible
/// consumer that a folder holds no files, that same instant.
#[test]
fn a_stitched_directory_lists_its_files_not_only_its_subdirectories() {
    let t = Tree::new();
    t.make(&["sub"], &["one.txt", "two.txt"]);

    stitch::directory(&t.space, &t.writer, t.root());

    assert_eq!(
        t.indexed_children(&t.root().to_string_lossy()),
        vec!["one.txt".to_string(), "sub".to_string(), "two.txt".to_string()],
        "a stitched directory's rows are its whole listing, files included"
    );
}

/// The other half of the stamp's story. A build whose exclusion policy changed
/// can't trust a row in the index, and during the phased window nothing else would
/// ever repair that: every phase would re-walk the whole scope and never re-stamp,
/// so the volume would never converge again.
#[test]
fn a_changed_exclusion_fingerprint_rebuilds_a_phased_index() {
    let drive = Drive::new(
        "phased-fingerprint",
        |root| {
            std::fs::create_dir_all(root.join("kept")).expect("dirs");
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();
    assert!(drive.meta("scan_completed_at").is_some(), "precondition: it completed");

    // A build with a different exclusion policy, as the index records it.
    {
        let conn = IndexStore::open_write_connection(&drive.db_path()).expect("write connection");
        IndexStore::update_meta(&conn, crate::indexing::store::EXCLUSION_POLICY_KEY, "some-older-policy")
            .expect("stamp an older policy");
        IndexStore::delete_meta(&conn, "scan_completed_at").expect("a partial index, as a relaunch would find it");
    }

    drive.restart();
    drive.wait_for_the_machine();

    assert_eq!(
        drive.meta(crate::indexing::store::EXCLUSION_POLICY_KEY).as_deref(),
        Some(crate::indexing::scanner::exclusion_policy_fingerprint().as_str()),
        "the rebuild re-stamps, so coverage answers mean something again"
    );
    assert!(
        drive.frontier(&drive.path("")).is_empty(),
        "and the rebuilt index converges"
    );
}

/// A truncating rescan under the machine would blank rows it is still writing and
/// make the sizes the user has been watching appear vanish again. `start_scan`'s
/// own reconcile-or-truncate predicate is what makes it a TRUNCATE: the index has
/// rows and no `scan_completed_at`, which is "a partial that never finished".
///
/// ⚠️ The guard has to hold BETWEEN frontier roots too, where no walk is running —
/// the stitch produces 50–150 roots per phase, so those gaps are most of the run.
/// That is why it asks whether the machine has WORK, not whether a walk is live.
#[test]
fn start_scan_refuses_while_a_phase_is_active() {
    let drive = Drive::new(
        "phased-refuses-rescan",
        |root| {
            // Enough ground that the machine is provably still working a moment
            // after it is handed the volume; the assertion below says so out loud
            // rather than passing vacuously if that ever stops being true.
            for outer in 0..20 {
                for inner in 0..20 {
                    std::fs::create_dir_all(root.join(format!("d{outer}/e{inner}"))).expect("dirs");
                }
            }
        },
        &[],
    );
    drive.start();

    // The machine holds the volume from the moment it is handed over until it has
    // nothing queued, so this is a refusal rather than a race.
    assert!(
        drive.index.status(drive.volume_id).is_ok_and(|status| status.scanning),
        "precondition: the machine has work the moment the volume is handed to it"
    );
    let epoch = drive.current_epoch();
    let outcome = crate::indexing::lifecycle::state::force_scan(drive.volume_id);
    assert!(outcome.is_ok(), "the request is answered, not an error: {outcome:?}");
    // The durable evidence, and the one that doesn't race the machine's own
    // event: every `start_scan` bumps `current_epoch` before it walks, so an
    // unchanged epoch means no second run got past the guard.
    assert_eq!(
        drive.current_epoch(),
        epoch,
        "❌ a second run started over the top of the machine, which would have truncated the index"
    );

    drive.wait_for_the_machine();
    assert!(
        drive.frontier(&drive.path("")).is_empty(),
        "and the machine finished what it was doing"
    );
    assert_eq!(drive.scans_started(), 1, "one run, start to finish");
}

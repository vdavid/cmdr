//! The descent rule, its premise, and the partition property it rests on.
//!
//! Every fixture here builds its `dir_stats` through the real aggregator rather
//! than writing `min_subtree_epoch` by hand, so the premise the rule depends on
//! (`min_subtree_epoch > 0` ⇒ `listed_epoch > 0`) holds by construction from the
//! canonical code instead of by the test's good manners.

use std::collections::{HashMap, HashSet};

use proptest::prelude::*;
use rusqlite::Connection;

use super::*;
use crate::indexing::aggregator::compute_all_aggregates;
use crate::indexing::scanner::exclusion_policy_fingerprint;
use crate::indexing::store::{EXCLUSION_POLICY_KEY, ROOT_ID};

// ── Fixture plumbing ─────────────────────────────────────────────────

/// A temp-file-backed index, already stamped with the current exclusion policy so
/// the tests exercise the descent rather than the policy gate.
fn open_temp_index() -> (Connection, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("coverage-test-index.db");
    let store = IndexStore::open(&db_path).expect("open store");
    let conn = IndexStore::open_write_connection(store.db_path()).expect("write connection");
    stamp_current_policy(&conn);
    (conn, dir)
}

/// Stamp the DB as built against the exclusion policy this build applies, which is
/// what a truncating full walk does.
fn stamp_current_policy(conn: &Connection) {
    IndexStore::update_meta(conn, EXCLUSION_POLICY_KEY, &exclusion_policy_fingerprint()).expect("stamp policy");
}

/// Insert a directory and return its id.
fn insert_dir(conn: &Connection, parent_id: i64, name: &str) -> i64 {
    IndexStore::insert_entry_v2(conn, parent_id, name, true, false, None, None, None, None).expect("insert dir")
}

/// Insert a file, so a fixture can prove files never reach the frontier.
fn insert_file(conn: &Connection, parent_id: i64, name: &str, size: u64) -> i64 {
    IndexStore::insert_entry_v2(conn, parent_id, name, false, false, Some(size), Some(size), None, None)
        .expect("insert file")
}

/// Mark directories listed at `epoch` and roll the aggregates up from them, which
/// is what a completed walk leaves behind.
fn list_and_aggregate(conn: &Connection, listed: &[i64], epoch: u64) {
    IndexStore::mark_dirs_listed(conn, listed, epoch).expect("mark listed");
    compute_all_aggregates(conn).expect("aggregate");
}

/// Run the descent and collect every directory's verdict, keyed by path.
fn verdicts(conn: &Connection, scope: &str) -> Vec<(Verdict, String)> {
    let mut seen = Vec::new();
    walk_coverage(conn, scope, scope, &mut |verdict, path| {
        seen.push((verdict, path.to_string()));
    })
    .expect("walk coverage");
    seen
}

/// Just the frontier and unreadable lists, sorted so assertions don't depend on
/// the descent's stack order.
fn coverage(conn: &Connection, scope: &str) -> CoverageMap {
    let mut map = coverage_for_scope(conn, scope, scope, CoverageDimension::Listing).expect("coverage for scope");
    map.frontier.sort();
    map.permission_denied.sort();
    map.declined.sort();
    map
}

// ── The premise the rule rests on ────────────────────────────────────

/// `min_subtree_epoch > 0` implies `listed_epoch > 0`, through the real aggregator.
///
/// The descent reads "covered" off `min_subtree_epoch` alone and never re-checks
/// `listed_epoch` for that case, so a row that claimed a covered subtree without
/// having been listed would make the descent skip genuinely uncovered ground with
/// no signal. Both writers of the column seed from the directory's own
/// `listed_epoch`, which is what makes that unrepresentable; this holds them to it.
#[test]
fn min_subtree_epoch_implies_listed() {
    let (conn, _dir) = open_temp_index();
    // A mix: listed leaves, listed interiors, and unlisted holes at three depths.
    let a = insert_dir(&conn, ROOT_ID, "a");
    let a_b = insert_dir(&conn, a, "b");
    let a_b_c = insert_dir(&conn, a_b, "c");
    let a_d = insert_dir(&conn, a, "d");
    let e = insert_dir(&conn, ROOT_ID, "e");
    let e_f = insert_dir(&conn, e, "f");
    // A branch that IS complete, so the vacuity guard below has something to count.
    let g = insert_dir(&conn, ROOT_ID, "g");
    let g_h = insert_dir(&conn, g, "h");
    insert_file(&conn, a_b, "file.txt", 10);

    list_and_aggregate(&conn, &[ROOT_ID, a, a_b, a_d, e, g, g_h], 4);
    // `a_b_c` and `e_f` stay unlisted, so their ancestors absorb to 0.

    let mut offenders = Vec::new();
    let mut stmt = conn
        .prepare(
            "SELECT e.id, e.listed_epoch, ds.min_subtree_epoch
             FROM entries e JOIN dir_stats ds ON ds.entry_id = e.id
             WHERE e.is_directory = 1",
        )
        .expect("prepare");
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, u64>(1)?, row.get::<_, u64>(2)?))
        })
        .expect("query");
    for row in rows {
        let (id, listed, mse) = row.expect("row");
        if mse > 0 && listed == 0 {
            offenders.push(id);
        }
    }
    assert!(
        offenders.is_empty(),
        "these dirs claim a covered subtree without having been listed: {offenders:?}"
    );
    // The fixture has to actually exercise both sides, or the assertion above is
    // vacuous on an all-zero table.
    let covered: u64 = conn
        .query_row("SELECT COUNT(*) FROM dir_stats WHERE min_subtree_epoch > 0", [], |r| {
            r.get(0)
        })
        .expect("count covered");
    assert!(covered >= 2, "fixture must produce covered rows, got {covered}");
    for unlisted in [a_b_c, e_f] {
        assert_eq!(
            IndexStore::get_listed_epoch_by_id(&conn, unlisted).expect("listed epoch"),
            Some(0),
            "fixture must keep an unlisted hole"
        );
    }
}

// ── The partition property ───────────────────────────────────────────

/// A generated directory tree: `(id, parent_id)` pairs plus per-directory
/// `listed` and `unreadable` flags. Ids are `1..=n` with `1` the scope root
/// (`ROOT_ID`), and every parent index is lower than its child's, so the shape is
/// always an acyclic tree rooted at the sentinel.
type GeneratedTree = Vec<(usize, bool, bool)>;

/// Trees of up to 24 directories, each with a parent among the ones before it.
fn tree_strategy() -> impl Strategy<Value = GeneratedTree> {
    (1usize..=24).prop_flat_map(|n| {
        let nodes: Vec<_> = (0..n)
            .map(|i| (0..=i.saturating_sub(1), any::<bool>(), any::<bool>()))
            .collect();
        nodes
    })
}

/// A generated tree as it exists in the DB, plus the model the assertions compare
/// the descent against.
struct Materialized {
    paths: HashMap<i64, String>,
    children: HashMap<i64, Vec<i64>>,
    listed: HashSet<i64>,
    unreadable: HashSet<i64>,
}

impl Materialized {
    /// The id whose path is `path`, panicking if the descent invented one.
    fn id_of(&self, path: &str) -> i64 {
        self.paths
            .iter()
            .find(|(_, p)| p.as_str() == path)
            .map(|(id, _)| *id)
            .expect("every reported path must be a directory in the tree")
    }

    /// Every id under `root`, inclusive.
    fn subtree(&self, root: i64) -> Vec<i64> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            out.push(id);
            if let Some(kids) = self.children.get(&id) {
                stack.extend(kids);
            }
        }
        out
    }
}

/// Build the generated tree in a real index.
fn materialize(conn: &Connection, tree: &GeneratedTree) -> Materialized {
    let mut ids = vec![ROOT_ID];
    let mut out = Materialized {
        paths: HashMap::from([(ROOT_ID, "/".to_string())]),
        children: HashMap::new(),
        listed: HashSet::new(),
        unreadable: HashSet::new(),
    };

    for (i, &(parent_index, is_listed, is_unreadable)) in tree.iter().enumerate() {
        // The first node IS the scope root (the sentinel), which already exists.
        let id = if i == 0 {
            ROOT_ID
        } else {
            let parent = ids[parent_index];
            let id = insert_dir(conn, parent, &format!("d{i}"));
            ids.push(id);
            out.paths.insert(id, join_path(&out.paths[&parent], &format!("d{i}")));
            out.children.entry(parent).or_default().push(id);
            id
        };
        if is_listed {
            out.listed.insert(id);
        }
        if is_unreadable {
            out.unreadable.insert(id);
            IndexStore::mark_dirs_unreadable(conn, &[id], Some(UnreadableCause::Denied)).expect("mark unreadable");
        }
    }
    let listed: Vec<i64> = out.listed.iter().copied().collect();
    list_and_aggregate(conn, &listed, 3);
    out
}

/// Run the descent over a materialized tree and pair each verdict with the id it
/// was reported for.
fn descend(conn: &Connection, model: &Materialized) -> Vec<(Verdict, i64)> {
    let mut out = Vec::new();
    walk_coverage(conn, "/", "/", &mut |verdict, path| {
        out.push((verdict, model.id_of(path)));
    })
    .expect("walk coverage");
    out
}

proptest! {
    // Each case opens a temp-file SQLite index and runs the real aggregator over
    // it, so a case is milliseconds rather than microseconds. 96 over a space of
    // ≤24-node trees still explores it thoroughly; the default 256 made these two
    // the slowest tests in the crate and starved them under the Linux lane's load.
    #![proptest_config(ProptestConfig::with_cases(96))]

    /// The verdicts partition the scope: every directory in it is accounted for
    /// exactly once, by exactly one of covered, listed, frontier, or unreadable.
    ///
    /// A cut owns its whole subtree; an interior "listed" node owns only itself,
    /// because it IS covered ground and only its descendants are in question. If
    /// this holds, "the index answers everything the frontier doesn't" is a fact
    /// about the tree rather than a hope, and the caller needs no deduplication
    /// between the two halves.
    #[test]
    fn coverage_partitions_the_subtree(tree in tree_strategy()) {
        let (conn, _dir) = open_temp_index();
        let model = materialize(&conn, &tree);

        let mut produced: Vec<i64> = Vec::new();
        for (verdict, id) in descend(&conn, &model) {
            match verdict {
                Verdict::Covered | Verdict::Frontier | Verdict::Unreadable(_) => produced.extend(model.subtree(id)),
                Verdict::Listed => produced.push(id),
            }
        }

        let produced_set: HashSet<i64> = produced.iter().copied().collect();
        prop_assert_eq!(
            produced.len(),
            produced_set.len(),
            "a directory was accounted for twice: {:?}",
            produced.iter().map(|id| &model.paths[id]).collect::<Vec<_>>()
        );
        let expected: HashSet<i64> = model.subtree(ROOT_ID).into_iter().collect();
        prop_assert_eq!(
            produced_set,
            expected,
            "the verdicts must account for exactly the scope's directories"
        );
    }

    /// Each verdict matches what its directory actually is.
    ///
    /// The partition alone is not enough: "the scope root is the whole frontier"
    /// partitions perfectly and is a useless answer, which is exactly the
    /// degenerate `min_subtree_epoch`-only rule two drafts of this design shipped.
    /// This is what catches it — a frontier cut has to be a directory nothing has
    /// listed, and a covered cut has to have every directory under it listed.
    #[test]
    fn every_verdict_matches_its_directory(tree in tree_strategy()) {
        let (conn, _dir) = open_temp_index();
        let model = materialize(&conn, &tree);

        for (verdict, id) in descend(&conn, &model) {
            let path = &model.paths[&id];
            match verdict {
                // Nothing under a covered cut may be unlisted, or the walk skips
                // ground nobody has read and the search silently comes back short.
                Verdict::Covered => {
                    for under in model.subtree(id) {
                        prop_assert!(
                            model.listed.contains(&under),
                            "\"{}\" is covered but \"{}\" under it was never listed",
                            path,
                            model.paths[&under]
                        );
                    }
                }
                // An interior node is itself covered ground.
                Verdict::Listed => prop_assert!(
                    model.listed.contains(&id),
                    "\"{path}\" was descended into as listed, but it never was"
                ),
                // Walking ground that IS listed is the degeneration.
                Verdict::Frontier => prop_assert!(
                    !model.listed.contains(&id),
                    "\"{path}\" is already listed, so handing it to the walk wastes the walk"
                ),
                Verdict::Unreadable(_) => {
                    prop_assert!(!model.listed.contains(&id), "\"{path}\" was listed, so it is readable");
                    prop_assert!(model.unreadable.contains(&id), "\"{path}\" carries no unreadable marker");
                }
            }
        }
    }
}

// ── Directed cases ───────────────────────────────────────────────────

/// One uncovered leaf deep in an otherwise-covered tree yields the leaf, not the
/// root. The degenerate `min_subtree_epoch`-only rule fails here: the leaf's zero
/// absorbs all the way to the scope root, so it would hand back `/`.
#[test]
fn a_single_uncovered_leaf_yields_the_leaf_not_the_root() {
    let (conn, _dir) = open_temp_index();
    let projects = insert_dir(&conn, ROOT_ID, "projects");
    let cmdr = insert_dir(&conn, projects, "cmdr");
    let docs = insert_dir(&conn, cmdr, "docs");
    let notes = insert_dir(&conn, cmdr, "notes");

    // Everything listed except `notes`.
    list_and_aggregate(&conn, &[ROOT_ID, projects, cmdr, docs], 2);
    assert_eq!(
        IndexStore::get_listed_epoch_by_id(&conn, notes).expect("epoch"),
        Some(0)
    );

    let map = coverage(&conn, "/");
    assert_eq!(map.frontier, vec!["/projects/cmdr/notes".to_string()]);
    assert!(map.permission_denied.is_empty() && map.declined.is_empty());
}

/// A volume the index has never seen hands back the scope root itself.
#[test]
fn a_cold_volume_yields_the_scope_root() {
    let (conn, _dir) = open_temp_index();
    let map = coverage(&conn, "/Users/dave/projects");
    assert_eq!(map.frontier, vec!["/Users/dave/projects".to_string()]);
}

/// An honest-stale gap on an otherwise complete index yields only that gap, and
/// the covered siblings are never descended into.
#[test]
fn an_honest_stale_gap_yields_only_that_gap() {
    let (conn, _dir) = open_temp_index();
    let users = insert_dir(&conn, ROOT_ID, "Users");
    let dave = insert_dir(&conn, users, "dave");
    let pictures = insert_dir(&conn, dave, "Pictures");
    let camera = insert_dir(&conn, pictures, "Camera");
    let library = insert_dir(&conn, dave, "Library");
    let abandoned = insert_dir(&conn, library, "Containers");
    insert_file(&conn, camera, "IMG_0001.heic", 4_000_000);

    // The walker abandoned `Containers` (never marked listed); everything else is
    // covered at epoch 5.
    list_and_aggregate(&conn, &[ROOT_ID, users, dave, pictures, camera, library], 5);
    assert_eq!(
        IndexStore::get_listed_epoch_by_id(&conn, abandoned).expect("epoch"),
        Some(0)
    );

    let map = coverage(&conn, "/");
    assert_eq!(map.frontier, vec!["/Users/dave/Library/Containers".to_string()]);

    // The covered branch is cut at the shallowest covered node, not walked through.
    let seen = verdicts(&conn, "/");
    let cut_at_pictures = seen
        .iter()
        .any(|(v, p)| *v == Verdict::Covered && p == "/Users/dave/Pictures");
    assert!(cut_at_pictures, "a covered sibling is one cut, not a descent: {seen:?}");
    assert!(
        !seen.iter().any(|(_, p)| p == "/Users/dave/Pictures/Camera"),
        "the descent must stop at a covered node: {seen:?}"
    );
}

/// A fully covered scope yields an empty frontier, and the one covered cut spans
/// every directory in it. Without the span assertion this passes on a no-op: an
/// implementation that returned early and looked at nothing would look identical.
#[test]
fn a_fully_covered_scope_yields_an_empty_frontier() {
    let (conn, _dir) = open_temp_index();
    let mut all = vec![ROOT_ID];
    // A 10-directory tree, three levels deep.
    for top in 0..3 {
        let top_id = insert_dir(&conn, ROOT_ID, &format!("top{top}"));
        all.push(top_id);
        for mid in 0..2 {
            let mid_id = insert_dir(&conn, top_id, &format!("mid{mid}"));
            all.push(mid_id);
            insert_file(&conn, mid_id, "leaf.txt", 1);
        }
    }
    assert_eq!(all.len(), 10, "fixture size, so the span assertion means something");
    list_and_aggregate(&conn, &all, 9);

    let map = coverage(&conn, "/");
    assert!(map.frontier.is_empty(), "nothing to walk: {map:?}");
    assert!(map.permission_denied.is_empty() && map.declined.is_empty());

    // Every directory was accounted for: the single covered cut is the scope root,
    // so its subtree IS the whole scope. Without the count below this passes on a
    // no-op — an implementation that looked at nothing would answer identically.
    let seen = verdicts(&conn, "/");
    assert_eq!(
        seen,
        vec![(Verdict::Covered, "/".to_string())],
        "one cut at the root, covering all {} directories",
        all.len()
    );
    let dirs_in_db: usize = conn
        .query_row("SELECT COUNT(*) FROM entries WHERE is_directory = 1", [], |r| {
            r.get::<_, i64>(0)
        })
        .expect("count dirs") as usize;
    assert_eq!(dirs_in_db, all.len(), "the cut spans every directory in the scope");
}

/// A directory a walk has tried and can't read is reported, not re-offered. Left
/// unmarked it would re-enter the frontier on every single search, a permanent
/// slow path with nothing to show the user for it.
#[test]
fn a_known_unreadable_dir_is_reported_rather_than_walked_again() {
    let (conn, _dir) = open_temp_index();
    let users = insert_dir(&conn, ROOT_ID, "Users");
    let dave = insert_dir(&conn, users, "dave");
    let documents = insert_dir(&conn, dave, "Documents");
    let _downloads = insert_dir(&conn, dave, "Downloads");

    list_and_aggregate(&conn, &[ROOT_ID, users, dave], 2);
    IndexStore::mark_dirs_unreadable(&conn, &[documents], Some(UnreadableCause::Denied)).expect("mark unreadable");

    let map = coverage(&conn, "/");
    assert_eq!(map.permission_denied, vec!["/Users/dave/Documents".to_string()]);
    assert!(
        map.declined.is_empty(),
        "a refusal is not a folder Cmdr declines to read"
    );
    assert_eq!(
        map.frontier,
        vec!["/Users/dave/Downloads".to_string()],
        "the readable sibling is still frontier"
    );

    // Clearing the mark (what a later successful listing does) puts it back in the
    // frontier without a rebuild.
    IndexStore::mark_dirs_unreadable(&conn, &[documents], None).expect("clear mark");
    let healed = coverage(&conn, "/");
    assert!(healed.permission_denied.is_empty());
    assert_eq!(
        healed.frontier,
        vec!["/Users/dave/Documents".to_string(), "/Users/dave/Downloads".to_string()]
    );
}

/// A directory Cmdr declines to read is reported APART from one it was refused.
///
/// The two are the same shape in the index and different sentences on screen: one
/// is a permission the user can grant, the other is a NAS snapshot tree nobody
/// walks on purpose. Telling them apart by folder name isn't an option, so the
/// cause is stored.
#[test]
fn a_declined_dir_is_reported_apart_from_a_refused_one() {
    let (conn, _dir) = open_temp_index();
    let share = insert_dir(&conn, ROOT_ID, "share");
    let snapshots = insert_dir(&conn, share, "@eaDir");
    let locked = insert_dir(&conn, share, "private");

    list_and_aggregate(&conn, &[ROOT_ID, share], 2);
    IndexStore::mark_dirs_unreadable(&conn, &[snapshots], Some(UnreadableCause::Declined)).expect("mark declined");
    IndexStore::mark_dirs_unreadable(&conn, &[locked], Some(UnreadableCause::Denied)).expect("mark denied");

    let map = coverage(&conn, "/");
    assert_eq!(
        map.declined,
        vec!["/share/@eaDir".to_string()],
        "a snapshot tree is ground Cmdr won't read, not ground it was refused"
    );
    assert_eq!(
        map.permission_denied,
        vec!["/share/private".to_string()],
        "and the refusal stays the half the user can act on"
    );
}

/// Ground a walk GAVE UP on is reported apart from both of its neighbours.
///
/// It shares "not the user's to fix" with a declined snapshot tree and "a walk did
/// try" with a refusal, and it is neither: Cmdr comes back to it on a backoff. So
/// it needs a bucket of its own, or the answer either offers Full Disk Access for a
/// wedged mount or calls a temporary hole a permanent policy.
#[test]
fn abandoned_ground_is_reported_apart_from_refused_and_declined_ground() {
    let (conn, _dir) = open_temp_index();
    let mnt = insert_dir(&conn, ROOT_ID, "mnt");
    let wedged = insert_dir(&conn, mnt, "phone");
    let snapshots = insert_dir(&conn, mnt, "@eaDir");
    let locked = insert_dir(&conn, mnt, "private");
    let _fine = insert_dir(&conn, mnt, "photos");

    list_and_aggregate(&conn, &[ROOT_ID, mnt], 2);
    IndexStore::mark_dirs_unreadable(&conn, &[wedged], Some(UnreadableCause::Abandoned)).expect("mark abandoned");
    IndexStore::mark_dirs_unreadable(&conn, &[snapshots], Some(UnreadableCause::Declined)).expect("mark declined");
    IndexStore::mark_dirs_unreadable(&conn, &[locked], Some(UnreadableCause::Denied)).expect("mark denied");

    let map = coverage(&conn, "/");
    assert_eq!(
        map.abandoned,
        vec!["/mnt/phone".to_string()],
        "a wedged mount is its own kind of nothing"
    );
    assert_eq!(map.declined, vec!["/mnt/@eaDir".to_string()]);
    assert_eq!(
        map.permission_denied,
        vec!["/mnt/private".to_string()],
        "❌ never here: this list offers Full Disk Access, which does nothing for a timeout"
    );
    assert_eq!(
        map.frontier,
        vec!["/mnt/photos".to_string()],
        "and the abandoned dir is no longer handed to every later search"
    );

    // What the retry backoff does: clear the cause, and the ground is offered again
    // with no rebuild.
    let cleared = IndexStore::clear_unreadable_cause(&conn, UnreadableCause::Abandoned).expect("clear");
    assert_eq!(cleared, 1, "only the abandoned row");
    let reopened = coverage(&conn, "/");
    assert_eq!(
        reopened.frontier,
        vec!["/mnt/phone".to_string(), "/mnt/photos".to_string()],
        "the retry reopens exactly the ground it gave up on"
    );
    assert_eq!(
        reopened.declined,
        vec!["/mnt/@eaDir".to_string()],
        "❌ and clears nothing else"
    );
    assert_eq!(reopened.permission_denied, vec!["/mnt/private".to_string()]);
}

/// Files never reach the frontier: coverage is a property of directories, and a
/// listed directory's files arrived with the listing.
#[test]
fn files_are_never_frontier() {
    let (conn, _dir) = open_temp_index();
    let docs = insert_dir(&conn, ROOT_ID, "docs");
    insert_file(&conn, ROOT_ID, "top.txt", 1);
    insert_file(&conn, docs, "nested.txt", 2);
    // `docs` is listed, root is not — so root descends and `docs` is covered.
    list_and_aggregate(&conn, &[ROOT_ID, docs], 1);

    let map = coverage(&conn, "/");
    assert!(map.frontier.is_empty(), "{map:?}");
}

/// An index built under an exclusion policy this build no longer applies claims no
/// coverage at all, whatever its epochs say. Removing a name from the policy
/// otherwise leaves the subtrees it used to skip permanently invisible.
#[test]
fn a_policy_mismatch_hands_back_the_whole_scope() {
    let (conn, _dir) = open_temp_index();
    let projects = insert_dir(&conn, ROOT_ID, "projects");
    list_and_aggregate(&conn, &[ROOT_ID, projects], 3);
    assert!(
        coverage(&conn, "/").frontier.is_empty(),
        "fixture must start fully covered"
    );

    IndexStore::update_meta(&conn, EXCLUSION_POLICY_KEY, "0000000000000000").expect("stale stamp");
    assert_eq!(coverage(&conn, "/").frontier, vec!["/".to_string()]);

    // And an index that was never stamped at all is treated the same way.
    IndexStore::delete_meta(&conn, EXCLUSION_POLICY_KEY).expect("drop stamp");
    assert_eq!(coverage(&conn, "/").frontier, vec!["/".to_string()]);
}

/// A scope below the volume root answers for that subtree only.
#[test]
fn a_scoped_query_answers_for_its_subtree_only() {
    let (conn, _dir) = open_temp_index();
    let users = insert_dir(&conn, ROOT_ID, "Users");
    let dave = insert_dir(&conn, users, "dave");
    let projects = insert_dir(&conn, dave, "projects");
    let cmdr = insert_dir(&conn, projects, "cmdr");
    let other = insert_dir(&conn, users, "guest");
    let _unwalked_elsewhere = insert_dir(&conn, other, "Downloads");

    list_and_aggregate(&conn, &[ROOT_ID, users, dave, projects, other], 2);

    let map = coverage(&conn, "/Users/dave/projects");
    assert_eq!(
        map.frontier,
        vec!["/Users/dave/projects/cmdr".to_string()],
        "the other user's gap is out of scope"
    );
    assert_ne!(cmdr, 0);
}

/// A parent chain deeper than the descent will follow is cut rather than walked
/// forever. Only corruption can produce one (the tree is built from a filesystem),
/// and a user-triggered query must not hang on it, so the node at the cap is
/// reported as frontier: worst case somebody re-walks covered ground.
#[test]
fn a_pathologically_deep_chain_is_cut_at_the_depth_cap() {
    let (conn, _dir) = open_temp_index();
    // A chain deeper than the cap, listed the whole way, with one unlisted leaf at
    // the bottom so every level has a reason to descend.
    let depth = MAX_DESCENT_DEPTH + 8;
    let mut parent = ROOT_ID;
    let mut listed = vec![ROOT_ID];
    let mut expected_cut = String::new();
    for level in 1..=depth {
        parent = insert_dir(&conn, parent, &format!("d{level}"));
        listed.push(parent);
        if level <= MAX_DESCENT_DEPTH {
            expected_cut.push_str(&format!("/d{level}"));
        }
    }
    // The unlisted leaf: without it the whole chain rolls up covered and the
    // descent stops at the root, which would pass this test for the wrong reason.
    insert_dir(&conn, parent, "unlisted");
    list_and_aggregate(&conn, &listed, 1);

    let map = coverage(&conn, "/");
    assert_eq!(
        map.frontier,
        vec![expected_cut],
        "the descent stops at the cap and hands that node to the walk"
    );
}

// ── The freshness token ──────────────────────────────────────────────

/// A walk that writes rows moves the token, so an answer computed before it can't
/// be mistaken for one computed after.
#[test]
fn writing_rows_moves_the_token() {
    let (conn, _dir) = open_temp_index();
    let before = coverage(&conn, "/").token;

    let projects = insert_dir(&conn, ROOT_ID, "projects");
    let after_insert = coverage(&conn, "/").token;
    assert_ne!(before, after_insert, "a new row is a new state");

    list_and_aggregate(&conn, &[ROOT_ID, projects], 1);
    let after_epoch_bump = {
        IndexStore::bump_current_epoch(&conn).expect("bump");
        coverage(&conn, "/").token
    };
    assert_ne!(after_insert, after_epoch_bump, "a continuity break is a new state");
}

/// Re-asking with nothing in between gives the same token, or a caller could never
/// tell "unchanged" from "changed".
#[test]
fn an_unchanged_index_reports_an_unchanged_token() {
    let (conn, _dir) = open_temp_index();
    insert_dir(&conn, ROOT_ID, "projects");
    assert_eq!(coverage(&conn, "/").token, coverage(&conn, "/").token);
}

// ── The measured exit criterion ──────────────────────────────────────

/// Time the frontier query over a REAL index, against the 50 ms warm budget.
///
/// Ignored by default: it needs a corpus no CI machine has. Point
/// `CMDR_COVERAGE_BENCH_DB` at a **copy** of an `index-root.db` and run
/// `cargo test -p cmdr-index --lib coverage::tests::measure -- --ignored --nocapture`.
///
/// ⚠️ It WRITES to the database it's given (adds the v15 column if missing, sets
/// the schema version, and stamps the current exclusion policy) so an index
/// captured under an older build can be measured without a ten-minute rescan.
/// Never point it at a live one.
///
/// The recorded run and what it means: `docs/notes/coverage-frontier-query-2026-08-05.md`.
#[test]
#[ignore = "needs a real multi-hundred-thousand-folder index; see the doc comment"]
#[allow(clippy::print_stdout, reason = "a benchmark prints its measurements")]
fn measure_frontier_query_on_a_real_index() {
    let Ok(db_path) = std::env::var("CMDR_COVERAGE_BENCH_DB") else {
        panic!("set CMDR_COVERAGE_BENCH_DB to a COPY of a real index-root.db");
    };
    let conn = IndexStore::open_write_connection(std::path::Path::new(&db_path)).expect("open the corpus");
    let has_column = conn
        .prepare("SELECT known_unreadable FROM entries LIMIT 1")
        .and_then(|mut s| s.query_row([], |_| Ok(())).or(Ok(())))
        .is_ok();
    if !has_column {
        conn.execute_batch("ALTER TABLE entries ADD COLUMN known_unreadable INTEGER NOT NULL DEFAULT 0")
            .expect("add the v15 column");
    }
    stamp_current_policy(&conn);

    let dirs: i64 = conn
        .query_row("SELECT COUNT(*) FROM entries WHERE is_directory = 1", [], |r| r.get(0))
        .expect("count dirs");
    let unlisted: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM entries WHERE is_directory = 1 AND listed_epoch = 0",
            [],
            |r| r.get(0),
        )
        .expect("count unlisted dirs");

    // Warm the page cache the way a second search would find it, then measure.
    let mut considered = 0usize;
    let first_started = std::time::Instant::now();
    let warm = coverage_for_scope(&conn, "/", "/", CoverageDimension::Listing).expect("warm-up run");
    let first_run = first_started.elapsed();
    let mut timings = Vec::new();
    for _ in 0..5 {
        considered = 0;
        let started = std::time::Instant::now();
        walk_coverage(&conn, "/", "/", &mut |_, _| considered += 1).expect("measured run");
        timings.push(started.elapsed());
    }
    timings.sort();

    println!(
        "corpus: directories {dirs}, of them unlisted {unlisted}\n\
         frontier: {} entries, unreadable: {}\n\
         directories considered per run: {considered}\n\
         first run (cache cold-ish): {first_run:?}\n\
         warm timings (5 runs, sorted): {timings:?}\n\
         median: {:?}",
        warm.frontier.len(),
        warm.permission_denied.len() + warm.declined.len(),
        timings[timings.len() / 2],
    );
    assert!(
        timings[timings.len() / 2] < std::time::Duration::from_millis(50),
        "the frontier query is the performance hinge: {:?} is over the 50 ms warm budget",
        timings[timings.len() / 2]
    );
}

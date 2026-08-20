//! End-to-end characterization of an incremental pass over a REAL index DB, run
//! under BOTH walks and differenced.
//!
//! Every other incremental test drives `incremental_rescore` with a hand-built
//! `WalkedFolders::synthetic`. These drive the whole pass instead — read the index,
//! rescore, write, then read the store back — so they pin the pass's OBSERVABLE
//! behaviour (which paths hold which score) rather than the shape of any one walk.
//!
//! Each scenario runs twice on two fresh volumes, once through the SCOPED walk and
//! once through the full O(dirs) walk, and the two stores must come out identical.
//! The full walk is the oracle: it is the implementation these transitions were
//! correct under before the scoped one existed, and it stays the fallback path. The
//! scenarios then assert the semantics on top, so a bug that both walks share still
//! gets caught. Depth: `DETAILS.md` § The scoped walk.

use std::collections::{HashMap, HashSet};

use super::recompute::{RescoreScope, dedupe_nested_origins, load_previous_markers, walk_for_incremental};
use super::scoped_walk::plan_incremental_batch;
use super::test_support::*;
use super::*;
use crate::indexing::store::{DirStatsById, IndexStore, ROOT_ID};

/// A fixed clock for every pass, so a score difference can only come from a signal
/// difference and never from the recency term moving between two runs.
const NOW_SECS: u64 = 1_700_000_000;

/// The home root every scenario builds under.
const HOME: &str = "/Users/test";

/// Which walk a pass reads the index through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WalkStrategy {
    /// What production does: the scoped walk, falling back to the full one when it
    /// can't stand in.
    Scoped,
    /// The full O(dirs) walk, always — the differential oracle and the fallback path.
    FullOnly,
}

const STRATEGIES: [WalkStrategy; 2] = [WalkStrategy::Scoped, WalkStrategy::FullOnly];

/// Every stored weight of one volume: path → (score, signals JSON).
type StoredWeights = HashMap<String, (f64, String)>;

/// One synthetic volume: a real `index-root.db` a test can mutate entry by entry,
/// plus the `importance-root.db` the passes write into.
struct TestVolume {
    dir: tempfile::TempDir,
    index: IndexStore,
    writer: ImportanceWriter,
    strategy: WalkStrategy,
    /// Absolute path → entry id, so a scenario can rename or delete by path.
    ids: HashMap<String, i64>,
    next_id: i64,
    /// How many passes took the full walk, so a scenario can assert WHICH path ran.
    full_walk_passes: usize,
    /// What the most recent incremental pass reported, so a scenario can assert how
    /// WIDE it was (`considered`), not only what it wrote.
    last_report: IncrementalReport,
}

impl TestVolume {
    fn new(strategy: WalkStrategy) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let index = IndexStore::open(&dir.path().join("index-root.db")).expect("open index");
        let writer = ImportanceWriter::spawn(&importance_db_path(dir.path(), ROOT_VOLUME_ID)).expect("writer");
        Self {
            dir,
            index,
            writer,
            strategy,
            ids: HashMap::new(),
            next_id: ROOT_ID + 1,
            full_walk_passes: 0,
            last_report: IncrementalReport::default(),
        }
    }

    /// Tell the index that `path`'s subtree holds `count` directories — the cheap
    /// exact measure a pass reads to decide whether an origin is too big to descend.
    /// A test sets the number directly rather than building 20,000 real directories.
    fn set_recursive_dir_count(&mut self, path: &str, count: u64) {
        let entry_id = *self.ids.get(path).expect("path was created");
        IndexStore::upsert_dir_stats_by_id(
            self.index.read_conn(),
            &[DirStatsById {
                entry_id,
                recursive_logical_size: 0,
                recursive_physical_size: 0,
                recursive_file_count: 0,
                recursive_dir_count: count,
                recursive_has_symlinks: false,
                min_subtree_epoch: 0,
            }],
        )
        .expect("dir stats");
    }

    /// Create `path` (and any missing ancestor) as a directory, returning its id.
    fn mkdir(&mut self, path: &str) -> i64 {
        if let Some(&id) = self.ids.get(path) {
            return id;
        }
        let (parent, name) = split_path(path);
        let parent_id = match parent {
            "" => ROOT_ID,
            p => self.mkdir(p),
        };
        let id = self.next_id;
        self.next_id += 1;
        IndexStore::insert_entry_v2_with_id(
            self.index.read_conn(),
            id,
            parent_id,
            name,
            true,
            false,
            None,
            None,
            Some(NOW_SECS - 3_600),
            None,
        )
        .expect("insert dir");
        self.ids.insert(path.to_string(), id);
        id
    }

    /// Create a file at `path` (ancestors auto-created).
    fn touch(&mut self, path: &str) {
        let (parent, name) = split_path(path);
        let parent_id = self.mkdir(parent);
        let id = self.next_id;
        self.next_id += 1;
        IndexStore::insert_entry_v2_with_id(
            self.index.read_conn(),
            id,
            parent_id,
            name,
            false,
            false,
            Some(10),
            Some(10),
            Some(NOW_SECS - 3_600),
            None,
        )
        .expect("insert file");
        self.ids.insert(path.to_string(), id);
    }

    /// Delete `path` and everything under it, as a real delete would.
    fn remove(&mut self, path: &str) {
        let id = *self.ids.get(path).expect("path was created");
        IndexStore::delete_subtree_by_id(self.index.read_conn(), id).expect("delete subtree");
        self.ids.retain(|p, _| p != path && !is_under(p, path));
    }

    /// Rename `path`'s last component to `new_name`, keeping the entry id (what an
    /// inode-preserving rename does in the live pipeline).
    fn rename(&mut self, path: &str, new_name: &str) {
        let id = *self.ids.get(path).expect("path was created");
        IndexStore::rename_entry(self.index.read_conn(), id, new_name).expect("rename");
        let (parent, _) = split_path(path);
        let new_path = format!("{parent}/{new_name}");
        // Re-key every path at or under the renamed folder.
        let moved: Vec<String> = self
            .ids
            .keys()
            .filter(|p| p.as_str() == path || is_under(p, path))
            .cloned()
            .collect();
        for old in moved {
            let id = self.ids.remove(&old).expect("just listed");
            let rest = &old[path.len()..];
            self.ids.insert(format!("{new_path}{rest}"), id);
        }
    }

    /// Run a FULL pass, the way `ScanCompleted` does: walk the whole index, score
    /// every folder, replace the table at a fresh generation.
    fn full_pass(&self) {
        let mut folders = walk_index_folders(self.index.read_conn(), HOME).expect("full walk");
        recompute_folders(
            &RecomputeInputs {
                writer: &self.writer,
                weights: &Weights::default(),
                home: HOME,
                now_secs: NOW_SECS,
                available: SignalSet::listing_only(),
                visits: &HashMap::new(),
                last_used: &HashMap::new(),
            },
            &mut folders,
        )
        .expect("full pass");
        self.writer.flush_blocking().expect("flush");
    }

    /// Run ONE incremental pass for `origins`, mirroring `run_incremental_blocking`:
    /// sanitize, de-duplicate, read the previous markers, walk, rescore, write.
    /// Returns how many rows the pass actually WROTE (the scope it considered is
    /// pinned by `incremental_tests.rs` instead).
    fn incremental(&mut self, origins: &[&str]) -> usize {
        let batch: Vec<String> = origins.iter().map(|p| (*p).to_string()).collect();
        let sanitized = sanitize_incremental_batch(&batch, HOME);
        self.last_report = IncrementalReport::default();
        if sanitized.is_empty() {
            return 0;
        }
        let previous = load_previous_markers(self.dir.path(), ROOT_VOLUME_ID, &sanitized);
        let conn = self.index.read_conn();
        let (mut folders, scope, plan) = match self.strategy {
            WalkStrategy::Scoped => walk_for_incremental(conn, HOME, &sanitized, &previous).expect("scoped walk"),
            WalkStrategy::FullOnly => (
                walk_index_folders(conn, HOME).expect("full walk"),
                RescoreScope::WithAncestors,
                plan_incremental_batch(conn, &sanitized).expect("plan"),
            ),
        };
        if scope == RescoreScope::WithAncestors {
            self.full_walk_passes += 1;
        }
        let (cleared, demoted) = plan.lists_for(scope);
        self.last_report = incremental_rescore(
            &IncrementalInputs {
                writer: &self.writer,
                weights: &Weights::default(),
                home: HOME,
                now_secs: NOW_SECS,
                available: SignalSet::listing_only(),
                visits: &HashMap::new(),
            },
            &mut folders,
            &cleared,
            scope,
            &demoted,
        )
        .expect("incremental")
        .report;
        self.writer.flush_blocking().expect("flush");
        self.last_report.written
    }

    /// Every stored weight, read back off disk.
    fn weights(&self) -> StoredWeights {
        let conn = crate::importance::store::open_read_connection(&importance_db_path(self.dir.path(), ROOT_VOLUME_ID))
            .expect("open store read");
        let mut stmt = conn
            .prepare("SELECT path, score, signals FROM weights")
            .expect("prepare");
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    (row.get::<_, f64>(1)?, row.get::<_, String>(2)?),
                ))
            })
            .expect("query");
        rows.map(|r| r.expect("row")).collect()
    }
}

/// Run `scenario` on a fresh volume under EACH walk strategy, assert the two stores
/// came out identical, and hand back the (shared) result for the semantic
/// assertions.
///
/// This is the differential: the full walk is the oracle, and any row the scoped
/// walk writes differently — path, score, or signal blob — fails here.
fn differential(scenario: impl Fn(&mut TestVolume)) -> StoredWeights {
    let mut outcomes: Vec<(WalkStrategy, StoredWeights, usize)> = Vec::new();
    for strategy in STRATEGIES {
        let mut volume = TestVolume::new(strategy);
        scenario(&mut volume);
        outcomes.push((strategy, volume.weights(), volume.full_walk_passes));
        volume.writer.shutdown();
    }
    assert_eq!(
        outcomes[0].1, outcomes[1].1,
        "the {:?} walk and the {:?} walk must leave the store identical",
        outcomes[0].0, outcomes[1].0
    );
    outcomes.remove(0).1
}

/// What a scoped-strategy run DID, for an assertion about how a pass ran rather than
/// what the store ended up holding.
struct PassTrace {
    /// How many of the run's passes fell back to the full walk.
    full_walk_passes: usize,
    /// What the run's last incremental pass reported.
    last_report: IncrementalReport,
}

/// Run `scenario` under the SCOPED walk alone and report how its passes ran.
fn scoped_trace(scenario: impl Fn(&mut TestVolume)) -> PassTrace {
    let mut volume = TestVolume::new(WalkStrategy::Scoped);
    scenario(&mut volume);
    let trace = PassTrace {
        full_walk_passes: volume.full_walk_passes,
        last_report: volume.last_report,
    };
    volume.writer.shutdown();
    trace
}

/// How many of a scoped-strategy run's passes fell back to the full walk.
fn full_walk_passes(scenario: impl Fn(&mut TestVolume)) -> usize {
    scoped_trace(scenario).full_walk_passes
}

/// Split an absolute path into `(parent, name)`; the parent of a top-level path is
/// `""` (the root sentinel).
fn split_path(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(pos) => (&path[..pos], &path[pos + 1..]),
        None => ("", path),
    }
}

/// Whether `path` sits strictly under `ancestor`.
fn is_under(path: &str, ancestor: &str) -> bool {
    path.strip_prefix(ancestor).is_some_and(|rest| rest.starts_with('/'))
}

/// Whether the stored row for `path` carries a project marker (at or below it).
/// Panics when there is no row — a floored folder has none by design.
fn has_marker(weights: &StoredWeights, path: &str) -> bool {
    let (_, signals) = weights.get(path).unwrap_or_else(|| panic!("no stored row for {path}"));
    parse(signals).has_project_marker
}

fn parse(signals: &str) -> crate::importance::FolderSignals {
    serde_json::from_str(signals).expect("signals parse")
}

/// Two independent project trees plus a floored one — the shape most scenarios start
/// from.
fn two_projects(v: &mut TestVolume) {
    for dir in [
        "/Users/test/projects/alpha/src/api",
        "/Users/test/projects/alpha/docs",
        "/Users/test/projects/beta/src",
        "/Users/test/projects/beta/node_modules/left-pad",
        "/Users/test/Documents/invoices",
    ] {
        v.mkdir(dir);
    }
    for file in [
        "/Users/test/projects/alpha/src/api/handlers.rs",
        "/Users/test/projects/alpha/src/api/notes.md",
        "/Users/test/projects/alpha/docs/readme.md",
        "/Users/test/projects/beta/src/index.ts",
        "/Users/test/projects/beta/node_modules/left-pad/index.js",
        "/Users/test/Documents/invoices/january.pdf",
    ] {
        v.touch(file);
    }
}

// ── The transitions an incremental pass has to get right ──────────────────

/// A marker created deep inside a subtree raises every ancestor above it.
///
/// This is the ONE signal that genuinely crosses a subtree boundary: `Cargo.toml`
/// lands in `alpha/src/api`, and `alpha/src`, `alpha`, `projects`, and the home all
/// have to read as project-adjacent afterwards. A scoped walk can't see those
/// ancestors, so it has to notice the flip and take the full walk instead.
#[test]
fn a_marker_created_inside_a_subtree_raises_its_ancestors() {
    let weights = differential(|v| {
        two_projects(v);
        v.full_pass();
        v.touch("/Users/test/projects/alpha/src/api/Cargo.toml");
        v.incremental(&["/Users/test/projects/alpha/src/api"]);
    });

    for raised in [
        "/Users/test/projects/alpha/src/api",
        "/Users/test/projects/alpha/src",
        "/Users/test/projects/alpha",
        "/Users/test/projects",
        "/Users/test",
    ] {
        assert!(
            has_marker(&weights, raised),
            "{raised} should read as project-adjacent now"
        );
    }
    assert!(
        !has_marker(&weights, "/Users/test/projects/beta"),
        "an unrelated sibling project is untouched"
    );
}

/// And the mirror image: deleting the last marker in a subtree lowers the ancestors
/// again. The `false` direction is the one a cache-the-previous-answer optimization
/// gets wrong, so it is pinned separately.
#[test]
fn a_marker_deleted_inside_a_subtree_lowers_its_ancestors() {
    let weights = differential(|v| {
        two_projects(v);
        v.touch("/Users/test/projects/alpha/src/api/Cargo.toml");
        v.full_pass();
        assert!(
            has_marker(&v.weights(), "/Users/test/projects/alpha"),
            "the marker is there to start"
        );
        v.remove("/Users/test/projects/alpha/src/api/Cargo.toml");
        v.incremental(&["/Users/test/projects/alpha/src/api"]);
    });

    for lowered in [
        "/Users/test/projects/alpha/src/api",
        "/Users/test/projects/alpha/src",
        "/Users/test/projects/alpha",
        "/Users/test/projects",
    ] {
        assert!(
            !has_marker(&weights, lowered),
            "{lowered} has no marker below it any more"
        );
    }
}

/// Both marker transitions are exactly what makes a pass take the full walk, and
/// nothing else in the everyday path does.
#[test]
fn only_a_marker_transition_costs_the_full_walk() {
    let created = full_walk_passes(|v| {
        two_projects(v);
        v.full_pass();
        v.touch("/Users/test/projects/alpha/src/api/Cargo.toml");
        v.incremental(&["/Users/test/projects/alpha/src/api"]);
    });
    assert_eq!(created, 1, "a marker appearing has to reach the ancestors above");

    let ordinary = full_walk_passes(|v| {
        two_projects(v);
        v.full_pass();
        v.touch("/Users/test/projects/alpha/src/api/extra.rs");
        v.incremental(&["/Users/test/projects/alpha/src/api"]);
        v.touch("/Users/test/Documents/invoices/february.pdf");
        v.incremental(&["/Users/test/Documents/invoices"]);
    });
    assert_eq!(ordinary, 0, "an ordinary file change is scoped, never a full walk");
}

/// A folder renamed TO `node_modules` floors its whole subtree: every row under it
/// goes, not only the renamed folder's own.
///
/// The live pipeline reports the renamed folder's PARENT as the origin (its listing
/// is what changed), so the downward expansion from the parent is the only thing
/// that revisits the subtree.
#[test]
fn a_folder_renamed_to_node_modules_floors_its_whole_subtree() {
    let weights = differential(|v| {
        two_projects(v);
        v.full_pass();
        assert!(
            v.weights().contains_key("/Users/test/projects/alpha/src/api"),
            "the subtree scores before the rename"
        );
        v.rename("/Users/test/projects/alpha/src", "node_modules");
        v.incremental(&["/Users/test/projects/alpha"]);
    });

    assert!(
        !weights.contains_key("/Users/test/projects/alpha/node_modules"),
        "the renamed folder floors by name"
    );
    assert!(
        !weights.contains_key("/Users/test/projects/alpha/node_modules/api"),
        "and so does everything under it — floor beats everything"
    );
    assert!(
        !weights.contains_key("/Users/test/projects/alpha/src"),
        "the old path leaves no orphan row"
    );
    assert!(
        weights.contains_key("/Users/test/projects/alpha/docs"),
        "a sibling under the same origin keeps its row"
    );
    assert!(
        weights.contains_key("/Users/test/projects/beta/src"),
        "an unrelated project is not dragged in"
    );
}

/// Renaming away from `node_modules` un-floors the whole subtree again: rows that
/// never existed appear.
#[test]
fn a_folder_renamed_away_from_node_modules_unfloors_its_whole_subtree() {
    let weights = differential(|v| {
        two_projects(v);
        v.full_pass();
        assert!(
            !v.weights()
                .contains_key("/Users/test/projects/beta/node_modules/left-pad"),
            "a floored folder has no row to start with"
        );
        v.rename("/Users/test/projects/beta/node_modules", "vendor");
        v.incremental(&["/Users/test/projects/beta"]);
    });

    assert!(
        weights.contains_key("/Users/test/projects/beta/vendor"),
        "the un-floored folder scores now"
    );
    assert!(
        weights.contains_key("/Users/test/projects/beta/vendor/left-pad"),
        "and so does its subtree"
    );
}

/// A change under an ALREADY-floored ancestor is a no-op: the batch gate drops it
/// before the pass costs anything, and nothing under the floor gains a row.
#[test]
fn a_change_under_an_already_floored_ancestor_writes_nothing() {
    let mut before = None;
    let weights = differential(|v| {
        two_projects(v);
        v.full_pass();
        let after_full = v.weights();
        v.touch("/Users/test/projects/beta/node_modules/left-pad/package.json");
        let count = v.incremental(&["/Users/test/projects/beta/node_modules/left-pad"]);
        assert_eq!(count, 0, "a batch of only floored paths costs nothing");
        assert_eq!(v.weights(), after_full, "and changes no row");
    });
    before.replace(weights.len());
    assert!(before.is_some_and(|n| n > 0), "the volume did score something");
}

/// A batch naming a high ancestor still produces exactly what a full pass would for
/// that subtree — and a batch of only the bare root is dropped.
#[test]
fn a_change_at_the_volume_root_stays_correct() {
    differential(|v| {
        two_projects(v);
        v.full_pass();
        let after_full = v.weights();

        assert_eq!(v.incremental(&["/"]), 0, "the bare root is never an origin");
        assert_eq!(v.weights(), after_full, "so nothing moves");

        // `/Users` IS a legitimate origin (something changed directly in it); it
        // just costs the whole volume. The result has to match the full pass.
        v.incremental(&["/Users"]);
        assert_eq!(
            v.weights(),
            after_full,
            "rescoring from the top reproduces the full pass"
        );
    });
}

/// A batch spanning several unrelated subtrees rescores each of them, and only
/// them. A nested origin rides along and must change nothing.
#[test]
fn a_batch_spanning_unrelated_subtrees_rescores_each() {
    let weights = differential(|v| {
        two_projects(v);
        v.full_pass();
        v.touch("/Users/test/projects/alpha/docs/guide.md");
        v.touch("/Users/test/Documents/invoices/february.pdf");
        v.touch("/Users/test/projects/beta/src/util.ts");
        v.incremental(&[
            "/Users/test/projects/alpha/docs",
            "/Users/test/Documents/invoices",
            "/Users/test/projects/beta",
            // Nested under the origin above; de-duplication drops it.
            "/Users/test/projects/beta/src",
        ]);
    });

    for path in [
        "/Users/test/projects/alpha/docs",
        "/Users/test/Documents/invoices",
        "/Users/test/projects/beta/src",
    ] {
        let (_, signals) = weights.get(path).unwrap_or_else(|| panic!("no row for {path}"));
        assert_eq!(parse(signals).file_count, 2, "{path} picked up its new file");
    }
}

/// An origin whose folder was deleted between the publish and the pass loses its
/// rows and re-adds nothing — the clear runs, the walk finds no folder to insert.
#[test]
fn an_origin_deleted_between_publish_and_pass_loses_its_rows() {
    let weights = differential(|v| {
        two_projects(v);
        v.full_pass();
        assert!(v.weights().contains_key("/Users/test/projects/alpha/src/api"));
        // The batch was published while the folder still existed; by the time the
        // throttled pass drains it, the folder is gone.
        v.remove("/Users/test/projects/alpha/src");
        v.incremental(&["/Users/test/projects/alpha/src"]);
    });

    assert!(
        !weights.contains_key("/Users/test/projects/alpha/src"),
        "the vanished origin's row is cleared"
    );
    assert!(
        !weights.contains_key("/Users/test/projects/alpha/src/api"),
        "and so is its subtree's"
    );
    assert!(
        weights.contains_key("/Users/test/projects/alpha/docs"),
        "a sibling outside the cleared subtree survives"
    );
}

/// An origin spelled in a different case than the index holds it behaves the SAME
/// under both walks, and never invents a second row under the batch's spelling.
///
/// **Known gap, PRE-existing and unchanged here:** the clear folds the path
/// (`path_folded` is the PK) while `is_in_changed_subtree` / `touched_folder_set`
/// compare bytes, so a case-variant origin clears rows that nothing re-adds. Both
/// walks lose the row identically, which is why this is a differential-only
/// scenario rather than a semantic assertion — pinning the current behaviour keeps
/// the scoped walk from drifting apart from the full one while the gap stands.
#[test]
fn an_origin_spelled_in_another_case_behaves_the_same_under_both_walks() {
    let weights = differential(|v| {
        two_projects(v);
        v.full_pass();
        v.touch("/Users/test/projects/alpha/docs/guide.md");
        v.incremental(&["/USERS/test/Projects/alpha/DOCS"]);
    });

    assert!(
        !weights.contains_key("/USERS/test/Projects/alpha/DOCS"),
        "no row under the batch's spelling — the index's own path is the identity"
    );
    assert!(
        weights.contains_key("/Users/test/projects/alpha/src/api"),
        "and nothing outside the named subtree is disturbed"
    );
}

/// De-duplication drops an origin nested under another, and keeps the batch's order
/// otherwise — the clear list and the insert set stay one slice.
/// A pass that finds nothing changed writes NOTHING — the idle case, end to end.
///
/// This is the treadmill `docs/notes/importance-treadmill-2026-08-04.md` measured:
/// on a real home, 99.88% of the rows a `$HOME`-origin pass rewrote every 60 s
/// carried a byte-identical signals blob. Both halves matter — the second pass
/// reports zero folders, AND the store it leaves behind is identical to the one the
/// first pass left, so the skip can't be hiding a write that mattered.
#[test]
fn a_repeated_pass_over_an_unchanged_subtree_writes_no_rows() {
    for strategy in STRATEGIES {
        let mut v = TestVolume::new(strategy);
        two_projects(&mut v);
        v.full_pass();

        // A real change: the folder gains a file, so its `file_count` signal moves.
        v.touch("/Users/test/projects/alpha/src/api/extra.md");
        let changed_pass = v.incremental(&["/Users/test/projects/alpha/src/api"]);
        let after_change = v.weights();

        // The same batch again, with nothing else touched.
        let idle_pass = v.incremental(&["/Users/test/projects/alpha/src/api"]);
        let after_idle = v.weights();
        v.writer.shutdown();

        assert!(changed_pass > 0, "{strategy:?}: a real change is written");
        assert_eq!(idle_pass, 0, "{strategy:?}: nothing changed, so nothing is written");
        assert_eq!(after_change, after_idle, "{strategy:?}: the store is untouched");
    }
}

/// An incremental that runs straight after a FULL pass writes nothing at all.
///
/// The production shape exactly: a full pass scores every folder, then dotfile churn
/// in `$HOME` makes it an origin every 60 s while no folder under it has actually
/// moved. Pinned separately from the test above because it exercises the
/// full-pass-then-incremental handoff rather than two incrementals in a row.
#[test]
fn an_incremental_over_an_untouched_volume_writes_no_rows() {
    for strategy in STRATEGIES {
        let mut v = TestVolume::new(strategy);
        two_projects(&mut v);
        v.full_pass();
        let after_full = v.weights();

        let written = v.incremental(&["/Users/test/projects"]);
        let after_incremental = v.weights();
        v.writer.shutdown();

        assert_eq!(written, 0, "{strategy:?}: the full pass already wrote every row");
        assert_eq!(
            after_full, after_incremental,
            "{strategy:?}: a no-op pass leaves the store alone"
        );
    }
}

/// Skipping the unchanged rows must not spare a STALE one: the clear and the insert
/// have to keep agreeing about the subtree.
///
/// The one way this optimization can lose (or keep) data. `docs` is deleted while
/// its sibling `src/api` is untouched, and both live under the same origin — so the
/// pass has to drop exactly one row and leave the other alone.
#[test]
fn a_deleted_folder_loses_its_row_while_its_unchanged_siblings_keep_theirs() {
    let weights = differential(|v| {
        two_projects(v);
        v.full_pass();
        v.remove("/Users/test/projects/alpha/docs");
        v.incremental(&["/Users/test/projects/alpha"]);
    });

    assert!(
        !weights.contains_key("/Users/test/projects/alpha/docs"),
        "the deleted folder's row is gone"
    );
    for kept in [
        "/Users/test/projects/alpha",
        "/Users/test/projects/alpha/src",
        "/Users/test/projects/alpha/src/api",
        "/Users/test/projects/beta/src",
    ] {
        // allowed-pluralize-noun: `kept` is a folder path, not a count.
        assert!(weights.contains_key(kept), "{kept} keeps its row");
    }
}

#[test]
fn nested_origins_collapse_to_their_outermost() {
    let batch: Vec<String> = ["/a/b/c", "/a/b", "/x", "/a/bc"]
        .iter()
        .map(|p| p.to_string())
        .collect();
    assert_eq!(
        dedupe_nested_origins(&batch, &HashSet::new()),
        vec!["/a/b".to_string(), "/x".to_string(), "/a/bc".to_string()],
        "`/a/b/c` is inside `/a/b`; `/a/bc` only shares a prefix, so it stays"
    );
}

/// A DEMOTED origin absorbs nothing: it never reads (or clears) its subtree, so an
/// origin nested under it still has its own work to do.
#[test]
fn a_demoted_origin_does_not_absorb_the_origins_nested_under_it() {
    let batch: Vec<String> = ["/a/b", "/a/b/c", "/a/b/c/d"].iter().map(|p| p.to_string()).collect();
    let demoted: HashSet<String> = ["/a/b".to_string()].into_iter().collect();
    assert_eq!(
        dedupe_nested_origins(&batch, &demoted),
        vec!["/a/b".to_string(), "/a/b/c".to_string()],
        "`/a/b` is demoted so `/a/b/c` survives, and `/a/b/c` then absorbs `/a/b/c/d`"
    );
}

/// The over-budget-origin half, split out for length (`demotion_tests.rs`).
mod demotion_tests;

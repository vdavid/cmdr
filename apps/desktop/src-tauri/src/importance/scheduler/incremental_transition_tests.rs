//! End-to-end characterization of an incremental pass over a REAL index DB.
//!
//! Every other incremental test drives `incremental_rescore` with a hand-built
//! `WalkedFolders::synthetic`. These drive the whole pass instead — read the index
//! through [`walk_for_incremental`], rescore, write, then read the store back — so
//! they pin the pass's OBSERVABLE behaviour (which paths hold which score) rather
//! than the shape of any one walk.
//!
//! That is deliberate: the walk is the pass's whole cost, so it will be replaced by
//! a cheaper one that reads only the changed subtrees. These tests are the oracle
//! that replacement must satisfy unchanged — they mutate a real index the way the
//! filesystem does (a marker appears, a folder is renamed to `node_modules`, an
//! origin is deleted before the pass runs) and assert what the store ends up
//! holding. See `docs/specs/scoped-incremental-walk.md`.

use std::collections::HashMap;

use super::test_support::*;
use super::*;
use crate::importance::scheduler::recompute::walk_for_incremental;
use crate::indexing::store::{IndexStore, ROOT_ID};

/// A fixed clock for every pass, so a score difference can only come from a signal
/// difference and never from the recency term moving between two runs.
const NOW_SECS: u64 = 1_700_000_000;

/// The home root every scenario builds under.
const HOME: &str = "/Users/test";

/// One synthetic volume: a real `index-root.db` a test can mutate entry by entry,
/// plus the `importance-root.db` the passes write into.
struct TestVolume {
    dir: tempfile::TempDir,
    index: IndexStore,
    writer: ImportanceWriter,
    /// Absolute path → entry id, so a scenario can rename or delete by path.
    ids: HashMap<String, i64>,
    next_id: i64,
}

impl TestVolume {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let index = IndexStore::open(&dir.path().join("index-root.db")).expect("open index");
        let writer = ImportanceWriter::spawn(&importance_db_path(dir.path(), ROOT_VOLUME_ID)).expect("writer");
        Self {
            dir,
            index,
            writer,
            ids: HashMap::new(),
            next_id: ROOT_ID + 1,
        }
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
    /// sanitize the batch, read the folders, rescore, write. Returns the row count
    /// the pass wrote.
    fn incremental(&self, origins: &[&str]) -> usize {
        let batch: Vec<String> = origins.iter().map(|p| (*p).to_string()).collect();
        let changed = sanitize_incremental_batch(&batch, HOME);
        if changed.is_empty() {
            return 0;
        }
        let mut folders = walk_for_incremental(self.index.read_conn(), HOME).expect("incremental walk");
        if folders.is_empty() {
            return 0;
        }
        let count = incremental_rescore(
            &IncrementalInputs {
                writer: &self.writer,
                weights: &Weights::default(),
                home: HOME,
                now_secs: NOW_SECS,
                available: SignalSet::listing_only(),
                visits: &HashMap::new(),
            },
            &mut folders,
            &changed,
        )
        .expect("incremental");
        self.writer.flush_blocking().expect("flush");
        count
    }

    /// Every stored weight, path → (score, signals JSON), read back off disk.
    fn weights(&self) -> HashMap<String, (f64, String)> {
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

    /// Whether the stored row for `path` carries a project marker (at or below it).
    /// Panics when there is no row — a floored folder has none by design.
    fn has_marker(&self, path: &str) -> bool {
        let weights = self.weights();
        let (_, signals) = weights.get(path).unwrap_or_else(|| panic!("no stored row for {path}"));
        serde_json::from_str::<crate::importance::FolderSignals>(signals)
            .expect("signals parse")
            .has_project_marker
    }

    fn shutdown(self) {
        self.writer.shutdown();
    }
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

/// A volume with two independent project trees plus a floored one, the shape most
/// scenarios start from.
fn two_projects() -> TestVolume {
    let mut v = TestVolume::new();
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
    v
}

// ── The transitions an incremental pass has to get right ──────────────────

/// A marker created deep inside a subtree raises every ancestor above it.
///
/// This is the ONE signal that genuinely crosses a subtree boundary: `.git` lands in
/// `alpha/src/api`, and `alpha/src`, `alpha`, `projects`, and the home all have to
/// read as project-adjacent afterwards.
#[test]
fn a_marker_created_inside_a_subtree_raises_its_ancestors() {
    let mut v = two_projects();
    v.full_pass();
    assert!(
        !v.has_marker("/Users/test/projects/alpha"),
        "no marker anywhere yet"
    );

    // A `Cargo.toml` appears; the origin is the directory whose listing changed.
    v.touch("/Users/test/projects/alpha/src/api/Cargo.toml");
    v.incremental(&["/Users/test/projects/alpha/src/api"]);

    for raised in [
        "/Users/test/projects/alpha/src/api",
        "/Users/test/projects/alpha/src",
        "/Users/test/projects/alpha",
        "/Users/test/projects",
        "/Users/test",
    ] {
        assert!(v.has_marker(raised), "{raised} should read as project-adjacent now");
    }
    assert!(
        !v.has_marker("/Users/test/projects/beta"),
        "an unrelated sibling project is untouched"
    );
    v.shutdown();
}

/// And the mirror image: deleting the last marker in a subtree lowers the ancestors
/// again. The `false` direction is the one a cache-the-previous-answer optimization
/// gets wrong, so it is pinned separately.
#[test]
fn a_marker_deleted_inside_a_subtree_lowers_its_ancestors() {
    let mut v = two_projects();
    v.touch("/Users/test/projects/alpha/src/api/Cargo.toml");
    v.full_pass();
    assert!(v.has_marker("/Users/test/projects/alpha"), "marker is there to start");

    v.remove("/Users/test/projects/alpha/src/api/Cargo.toml");
    v.incremental(&["/Users/test/projects/alpha/src/api"]);

    for lowered in [
        "/Users/test/projects/alpha/src/api",
        "/Users/test/projects/alpha/src",
        "/Users/test/projects/alpha",
        "/Users/test/projects",
    ] {
        assert!(!v.has_marker(lowered), "{lowered} has no marker below it any more");
    }
    v.shutdown();
}

/// A folder renamed TO `node_modules` floors its whole subtree: every row under it
/// goes, not only the renamed folder's own.
///
/// The live pipeline reports the renamed folder's PARENT as the origin (its listing
/// is what changed), so the downward expansion from the parent is the only thing
/// that revisits the subtree.
#[test]
fn a_folder_renamed_to_node_modules_floors_its_whole_subtree() {
    let mut v = two_projects();
    v.full_pass();
    assert!(
        v.weights().contains_key("/Users/test/projects/alpha/src/api"),
        "the subtree scores before the rename"
    );

    v.rename("/Users/test/projects/alpha/src", "node_modules");
    v.incremental(&["/Users/test/projects/alpha"]);

    let weights = v.weights();
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
    v.shutdown();
}

/// Renaming away from `node_modules` un-floors the whole subtree again: rows that
/// never existed appear.
#[test]
fn a_folder_renamed_away_from_node_modules_unfloors_its_whole_subtree() {
    let mut v = two_projects();
    v.full_pass();
    assert!(
        !v.weights().contains_key("/Users/test/projects/beta/node_modules/left-pad"),
        "a floored folder has no row to start with"
    );

    v.rename("/Users/test/projects/beta/node_modules", "vendor");
    v.incremental(&["/Users/test/projects/beta"]);

    let weights = v.weights();
    assert!(
        weights.contains_key("/Users/test/projects/beta/vendor"),
        "the un-floored folder scores now"
    );
    assert!(
        weights.contains_key("/Users/test/projects/beta/vendor/left-pad"),
        "and so does its subtree"
    );
    v.shutdown();
}

/// A change under an ALREADY-floored ancestor is a no-op: the batch gate drops it
/// before the pass costs anything, and nothing under the floor gains a row.
#[test]
fn a_change_under_an_already_floored_ancestor_writes_nothing() {
    let mut v = two_projects();
    v.full_pass();
    let before = v.weights();

    v.touch("/Users/test/projects/beta/node_modules/left-pad/package.json");
    let count = v.incremental(&["/Users/test/projects/beta/node_modules/left-pad"]);

    assert_eq!(count, 0, "a batch of only floored paths costs nothing");
    assert_eq!(v.weights(), before, "and changes no row");
    v.shutdown();
}

/// A batch naming a high ancestor still produces exactly what a full pass would for
/// that subtree — and a batch of only the bare root is dropped.
#[test]
fn a_change_at_the_volume_root_stays_correct() {
    let v = two_projects();
    v.full_pass();
    let after_full = v.weights();

    assert_eq!(v.incremental(&["/"]), 0, "the bare root is never an origin");
    assert_eq!(v.weights(), after_full, "so nothing moves");

    // `/Users` IS a legitimate origin (something changed directly in it); it just
    // costs the whole volume. The result has to match the full pass exactly.
    v.incremental(&["/Users"]);
    assert_eq!(
        v.weights(),
        after_full,
        "rescoring from the top reproduces the full pass"
    );
    v.shutdown();
}

/// A batch spanning several unrelated subtrees rescores each of them, and only
/// them.
#[test]
fn a_batch_spanning_unrelated_subtrees_rescores_each() {
    let mut v = two_projects();
    v.full_pass();

    v.touch("/Users/test/projects/alpha/docs/guide.md");
    v.touch("/Users/test/Documents/invoices/february.pdf");
    v.touch("/Users/test/projects/beta/src/util.ts");
    v.incremental(&[
        "/Users/test/projects/alpha/docs",
        "/Users/test/Documents/invoices",
        "/Users/test/projects/beta/src",
    ]);

    let weights = v.weights();
    for path in [
        "/Users/test/projects/alpha/docs",
        "/Users/test/Documents/invoices",
        "/Users/test/projects/beta/src",
    ] {
        let (_, signals) = weights.get(path).unwrap_or_else(|| panic!("no row for {path}"));
        let parsed: crate::importance::FolderSignals = serde_json::from_str(signals).expect("signals");
        assert_eq!(parsed.file_count, 2, "{path} picked up its new file");
    }
    v.shutdown();
}

/// An origin whose folder was deleted between the publish and the pass loses its
/// rows and re-adds nothing — the clear runs, the walk finds no folder to insert.
#[test]
fn an_origin_deleted_between_publish_and_pass_loses_its_rows() {
    let mut v = two_projects();
    v.full_pass();
    assert!(v.weights().contains_key("/Users/test/projects/alpha/src/api"));

    // The batch was published while the folder still existed; by the time the
    // throttled pass drains it, the folder is gone.
    v.remove("/Users/test/projects/alpha/src");
    v.incremental(&["/Users/test/projects/alpha/src"]);

    let weights = v.weights();
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
    v.shutdown();
}

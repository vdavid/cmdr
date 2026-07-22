//! Correctness tests for the non-destructive reconcile rescan.
//!
//! These drive the REAL `reconciler::reconcile_subtree` against REAL on-disk temp trees and a real
//! store + writer, to prove the correctness half of the gate:
//!
//!  1. Reconcile handles add / remove / modify / dir↔file type-change, updating dir_stats +
//!     min_subtree_epoch (the diff the full network rescan routes through).
//!  2. An INTERRUPTED reconcile (cancel mid-walk) followed by a later COMPLETE reconcile leaves NO
//!     orphan rows and NO ghost sizes — across REPEATED interrupt→reconcile cycles. Specifically: a dir
//!     deleted on the live side while the reconcile was interrupted before reaching it must NOT linger
//!     as an orphan.
//!  3. Epoch re-stamp gives partial-rescan granularity (reconcile /a but not /b → /a fresh, /b stale).
//!
//! Cheap; run in CI normally (no `#[ignore]`).
//!
//! Reconcile root note: `reconcile_subtree` requires the root path's entry to exist (it auto-creates
//! the LEAF root via its parent, but the parent chain must resolve). `non_excluded_tempdir` lives under
//! CWD; we seed the full path chain to the tree root via `ensure_path_in_db_test`, mirroring what a full
//! scan does before live events.

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};

    use rusqlite::Connection;

    use crate::indexing::paths::firmlinks;
    use crate::indexing::reconcile::reconciler::reconcile_subtree;
    use crate::indexing::store::{self, IndexStore, ROOT_ID};
    use crate::indexing::writer::{IndexWriter, WriteMessage};

    // ── Harness ──────────────────────────────────────────────────────

    struct Harness {
        writer: IndexWriter,
        conn: Connection,
        db_path: PathBuf,
        _dir: tempfile::TempDir,
    }

    fn setup() -> Harness {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("reconcile-correctness.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");
        let conn = IndexStore::open_write_connection(&db_path).expect("open WAL conn");
        // Seed current_epoch = 1 so the first reconcile stamps a real value.
        writer
            .send(WriteMessage::UpdateMeta {
                key: store::CURRENT_EPOCH_KEY.to_string(),
                value: "1".to_string(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();
        Harness {
            writer,
            conn,
            db_path,
            _dir: dir,
        }
    }

    /// A tree root under CWD (not /tmp — excluded on Linux, symlinked on macOS).
    fn tree_root() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("cmdr_reconcile_")
            .tempdir_in(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
            .expect("tempdir in cwd")
    }

    /// Ensure every component of `abs_path` exists in the DB as a directory entry,
    /// walking root→leaf (mirrors what a full scan does before live events). Syncs
    /// the writer's next_id.
    fn ensure_path_in_db(h: &Harness, abs_path: &str) {
        let conn = IndexStore::open_write_connection(&h.db_path).unwrap();
        let mut current_id = ROOT_ID;
        for component in abs_path.strip_prefix('/').unwrap_or(abs_path).split('/') {
            if component.is_empty() {
                continue;
            }
            match IndexStore::resolve_component(&conn, current_id, component).unwrap() {
                Some(id) => current_id = id,
                None => {
                    current_id =
                        IndexStore::insert_entry_v2(&conn, current_id, component, true, false, None, None, None, None)
                            .unwrap();
                }
            }
        }
        let db_next_id = IndexStore::get_next_id(&conn).unwrap();
        h.writer.next_id().fetch_max(db_next_id, Ordering::Relaxed);
    }

    fn norm(p: &Path) -> String {
        firmlinks::normalize_path(&p.to_string_lossy())
    }

    fn resolve(h: &Harness, p: &Path) -> Option<i64> {
        store::resolve_path(&h.conn, &norm(p)).unwrap()
    }

    fn dir_stats(h: &Harness, id: i64) -> Option<store::DirStatsById> {
        IndexStore::get_dir_stats_by_id(&h.conn, id).unwrap()
    }

    fn min_epoch(h: &Harness, p: &Path) -> u64 {
        let id = resolve(h, p).unwrap_or_else(|| panic!("path not in DB: {}", p.display()));
        dir_stats(h, id).map(|s| s.min_subtree_epoch).unwrap_or(0)
    }

    /// Orphan check: every non-sentinel entry's parent_id must reference an existing directory.
    /// A non-exhaustive interrupted reconcile is exactly what could leave these.
    fn assert_no_orphans(h: &Harness) {
        let orphans: Vec<(i64, i64, String)> = {
            let mut stmt = h
                .conn
                .prepare(
                    "SELECT e.id, e.parent_id, e.name FROM entries e
                     WHERE e.parent_id != 0
                       AND NOT EXISTS (SELECT 1 FROM entries p WHERE p.id = e.parent_id AND p.is_directory = 1)",
                )
                .unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };
        assert!(orphans.is_empty(), "orphaned entries: {orphans:?}");
    }

    /// Run a full (uninterrupted) reconcile and flush.
    fn reconcile_full(h: &Harness, root: &Path) {
        let cancelled = AtomicBool::new(false);
        reconcile_subtree(
            root,
            &crate::indexing::IndexPathSpace::root(),
            &h.conn,
            &h.writer,
            &cancelled,
        )
        .expect("reconcile");
        h.writer.flush_blocking().unwrap();
    }

    /// Bump the volume epoch (a continuity break would do this before a rescan).
    fn bump_epoch(h: &Harness) -> u64 {
        {
            let wconn = IndexStore::open_write_connection(&h.db_path).unwrap();
            IndexStore::bump_current_epoch(&wconn).unwrap()
        }
    }

    // ── 1. Basic diff correctness: add / remove / modify / type-change ──

    #[test]
    fn reconcile_handles_add_remove_modify_typechange() {
        let h = setup();
        let root = tree_root();
        let root_path = root.path();
        ensure_path_in_db(&h, &norm(root_path));

        // Initial tree on disk.
        std::fs::create_dir(root_path.join("sub")).unwrap();
        std::fs::write(root_path.join("sub/keep.txt"), b"keep").unwrap();
        std::fs::write(root_path.join("sub/modify.txt"), b"orig").unwrap();
        std::fs::write(root_path.join("sub/remove.txt"), b"gone-soon").unwrap();
        std::fs::create_dir(root_path.join("typeswap")).unwrap(); // dir, will become a file

        // Epoch 1: initial reconcile builds the index from disk.
        reconcile_full(&h, root_path);
        let sub = root_path.join("sub");
        let sub_id = resolve(&h, &sub).expect("sub indexed");
        let stats0 = dir_stats(&h, sub_id).expect("sub stats");
        assert_eq!(stats0.recursive_file_count, 3, "3 files initially");
        assert!(stats0.min_subtree_epoch > 0, "fully listed ⇒ exact");

        // Mutate disk: add, modify (size+mtime), remove, dir→file type change.
        std::fs::write(root_path.join("sub/added.txt"), b"new file here").unwrap();
        std::fs::write(root_path.join("sub/modify.txt"), b"a much longer body now").unwrap();
        std::fs::remove_file(root_path.join("sub/remove.txt")).unwrap();
        std::fs::remove_dir(root_path.join("typeswap")).unwrap();
        std::fs::write(root_path.join("typeswap"), b"now a file").unwrap();

        // Epoch 2 reconcile.
        bump_epoch(&h);
        reconcile_full(&h, root_path);

        let stats1 = dir_stats(&h, sub_id).expect("sub stats after");
        assert_eq!(
            stats1.recursive_file_count, 3,
            "added 1, removed 1 ⇒ still 3 files (keep, modify, added)"
        );
        // remove.txt gone.
        assert!(
            resolve(&h, &root_path.join("sub/remove.txt")).is_none(),
            "removed file gone"
        );
        // added.txt present.
        assert!(
            resolve(&h, &root_path.join("sub/added.txt")).is_some(),
            "added file present"
        );
        // typeswap is now a FILE.
        let typeswap_id = resolve(&h, &root_path.join("typeswap")).expect("typeswap present");
        let row = IndexStore::get_entry_by_id(&h.conn, typeswap_id).unwrap().unwrap();
        assert!(!row.is_directory, "typeswap changed dir→file");

        // min_subtree_epoch re-stamped to the new epoch (fully covered again).
        let root_id = resolve(&h, root_path).unwrap();
        assert!(
            dir_stats(&h, root_id).unwrap().min_subtree_epoch >= stats0.min_subtree_epoch,
            "coverage maintained after reconcile"
        );
        assert_no_orphans(&h);

        h.writer.shutdown();
    }

    // ── 3. Epoch re-stamp gives partial-rescan granularity ──

    #[test]
    fn partial_reconcile_keeps_a_fresh_b_stale() {
        let h = setup();
        let root = tree_root();
        let root_path = root.path();
        ensure_path_in_db(&h, &norm(root_path));

        std::fs::create_dir(root_path.join("a")).unwrap();
        std::fs::write(root_path.join("a/file.txt"), b"a body").unwrap();
        std::fs::create_dir(root_path.join("b")).unwrap();
        std::fs::write(root_path.join("b/file.txt"), b"b body").unwrap();

        // Epoch 1: index both.
        reconcile_full(&h, root_path);
        let e1_a = min_epoch(&h, &root_path.join("a"));
        let e1_b = min_epoch(&h, &root_path.join("b"));
        assert_eq!(e1_a, e1_b, "both fresh at epoch 1");
        assert!(e1_a > 0);

        // Continuity break: bump epoch. Now everything is stale (epoch < current).
        let current = bump_epoch(&h);
        assert_eq!(current, 2);

        // Reconcile ONLY /a.
        reconcile_full(&h, &root_path.join("a"));

        let a_id = resolve(&h, &root_path.join("a")).unwrap();
        let b_id = resolve(&h, &root_path.join("b")).unwrap();
        let a_epoch = dir_stats(&h, a_id).unwrap().min_subtree_epoch;
        let b_epoch = dir_stats(&h, b_id).unwrap().min_subtree_epoch;

        assert_eq!(a_epoch, current, "/a re-listed ⇒ fresh at current epoch {current}");
        assert_eq!(b_epoch, e1_b, "/b untouched ⇒ still at the old epoch (stale)");
        assert!(b_epoch < current, "/b is stale relative to current epoch");

        h.writer.shutdown();
    }

    // ── 2. Interrupted reconcile + complete reconcile ⇒ no orphans/ghosts ──

    // Interruption model: `reconcile_subtree` checks `cancelled` at the top of each dir pop. A
    // PRE-TRIPPED flag (`true` from the start) makes it stop right after listing the ROOT dir — the
    // root's children are pushed onto the queue but none are visited/diffed. That is the worst case for
    // orphan-freedom: a subtree deleted on disk is NOT swept by this interrupted pass, so the next
    // complete pass must heal it. Pre-tripping keeps the test fully deterministic (no thread timing).

    /// THE key gate test. Build a tree, index it fully (epoch 1). Then, simulating repeated
    /// disconnect→reconnect→reconcile cycles: on each cycle we (a) mutate the disk (including deleting a
    /// whole subdir), (b) bump the epoch, (c) run a reconcile that is CANCELLED partway, then later (d)
    /// run a COMPLETE reconcile. After all cycles, assert: no orphan rows, and the index byte-for-byte
    /// matches a fresh-from-scratch index of the final disk state (no ghost sizes).
    #[test]
    fn repeated_interrupted_then_complete_reconcile_leaves_no_orphans_or_ghosts() {
        let h = setup();
        let root = tree_root();
        let root_path = root.path();
        ensure_path_in_db(&h, &norm(root_path));

        // Wide-ish initial tree so an interrupted walk leaves real work undone.
        for i in 0..8 {
            let d = root_path.join(format!("d{i}"));
            std::fs::create_dir(&d).unwrap();
            for j in 0..5 {
                std::fs::write(d.join(format!("f{j}.txt")), format!("body {i}-{j}")).unwrap();
            }
            // A nested subdir under each, so the walk has depth to leave unvisited.
            let nested = d.join("nested");
            std::fs::create_dir(&nested).unwrap();
            std::fs::write(nested.join("deep.txt"), b"deep").unwrap();
        }

        // Epoch 1: full index.
        reconcile_full(&h, root_path);
        assert_no_orphans(&h);

        // 3 cycles of: mutate (incl. delete a subtree) → bump → interrupted reconcile → complete reconcile.
        for cycle in 0..3 {
            // (a) Mutate disk. Critically: DELETE a whole subdir on the live side. If an interrupted
            //     reconcile never reaches its parent, the stale rows could linger as an orphan/ghost.
            let victim = root_path.join(format!("d{cycle}"));
            if victim.exists() {
                std::fs::remove_dir_all(&victim).unwrap();
            }
            // Also add a fresh dir each cycle and modify a file.
            let fresh = root_path.join(format!("fresh{cycle}"));
            std::fs::create_dir(&fresh).unwrap();
            std::fs::write(fresh.join("new.txt"), format!("fresh body {cycle}")).unwrap();
            let modpath = root_path.join("d7/f0.txt");
            if modpath.exists() {
                std::fs::write(&modpath, format!("modified in cycle {cycle} aaaaaaaaaa")).unwrap();
            }

            // (b) Continuity break.
            bump_epoch(&h);

            // (c) Interrupted reconcile: cancel pre-tripped so it stops right after listing the root
            //     (children pushed, but none of them visited). This is the worst case: the deleted
            //     subtree's rows are NOT swept by this pass.
            let pretripped = AtomicBool::new(true);
            let _ = reconcile_subtree(
                root_path,
                &crate::indexing::IndexPathSpace::root(),
                &h.conn,
                &h.writer,
                &pretripped,
            );
            h.writer.flush_blocking().unwrap();
            // The root itself got listed+marked; its children diffs did not run. Deleted subtree rows
            // may still be present here — that's the hazard the next complete pass must heal.

            // (d) Complete reconcile.
            reconcile_full(&h, root_path);

            // After a COMPLETE pass, the deleted subtree must be gone and no orphans remain.
            assert!(
                resolve(&h, &victim).is_none(),
                "cycle {cycle}: deleted subtree d{cycle} still in index after complete reconcile (ORPHAN/GHOST)"
            );
            assert_no_orphans(&h);
        }

        // Final oracle: the reconciled index must match a fresh-from-scratch index of the same disk.
        let fresh_h = setup();
        ensure_path_in_db(&fresh_h, &norm(root_path));
        reconcile_full(&fresh_h, root_path);

        assert_index_trees_match(&h, &fresh_h, root_path);

        h.writer.shutdown();
        fresh_h.writer.shutdown();
    }

    /// Compare the reconciled DB against a fresh-from-scratch DB of the same disk tree: same set of
    /// (path) entries, same dir_stats sizes/counts. A ghost size or orphan would diverge here.
    fn assert_index_trees_match(a: &Harness, b: &Harness, root: &Path) {
        // Walk both DBs from the tree root and compare the (relative path -> kind+size) maps.
        let map_a = collect_subtree(a, root);
        let map_b = collect_subtree(b, root);

        let keys_a: HashSet<&String> = map_a.keys().collect();
        let keys_b: HashSet<&String> = map_b.keys().collect();
        let only_a: Vec<&&String> = keys_a.difference(&keys_b).collect();
        let only_b: Vec<&&String> = keys_b.difference(&keys_a).collect();
        assert!(
            only_a.is_empty() && only_b.is_empty(),
            "entry-set mismatch (reconciled-only: {only_a:?}, fresh-only: {only_b:?})"
        );
        for (k, va) in &map_a {
            let vb = &map_b[k];
            assert_eq!(va, vb, "entry {k}: reconciled={va:?} fresh={vb:?}");
        }
    }

    /// Collect (relative path -> (is_dir, recursive_logical_size)) for every entry under `root`.
    fn collect_subtree(h: &Harness, root: &Path) -> std::collections::HashMap<String, (bool, u64)> {
        let root_id = resolve(h, root).expect("root in DB");
        let mut out = std::collections::HashMap::new();
        let mut stack = vec![(root_id, String::new())];
        while let Some((id, prefix)) = stack.pop() {
            let children = IndexStore::list_children_on(id, &h.conn).unwrap();
            for c in children {
                let rel = if prefix.is_empty() {
                    c.name.clone()
                } else {
                    format!("{prefix}/{}", c.name)
                };
                let size = if c.is_directory {
                    dir_stats(h, c.id).map(|s| s.recursive_logical_size).unwrap_or(0)
                } else {
                    c.logical_size.unwrap_or(0)
                };
                out.insert(rel.clone(), (c.is_directory, size));
                if c.is_directory {
                    stack.push((c.id, rel));
                }
            }
        }
        out
    }

    // ── 2b. The specific probe: interrupted-before-reaching a deleted dir, single cycle ──

    /// Isolates the exact concern from the gate brief: a dir deleted on the live side while the
    /// reconcile was interrupted BEFORE reaching it. Does it linger as an orphan dragging the parent's
    /// min_subtree_epoch to incomplete forever? Answer this directly.
    #[test]
    fn deleted_dir_unreached_by_interrupted_reconcile_heals_on_next_complete() {
        let h = setup();
        let root = tree_root();
        let root_path = root.path();
        ensure_path_in_db(&h, &norm(root_path));

        std::fs::create_dir(root_path.join("alpha")).unwrap();
        std::fs::write(root_path.join("alpha/a.txt"), b"alpha body").unwrap();
        std::fs::create_dir(root_path.join("beta")).unwrap();
        std::fs::write(root_path.join("beta/b.txt"), b"beta body").unwrap();

        reconcile_full(&h, root_path);
        let root_id = resolve(&h, root_path).unwrap();
        assert!(
            dir_stats(&h, root_id).unwrap().min_subtree_epoch > 0,
            "covered after first pass"
        );

        // Delete beta on disk. Then interrupt a reconcile so it never visits the root's children.
        std::fs::remove_dir_all(root_path.join("beta")).unwrap();
        bump_epoch(&h);

        let pretripped = AtomicBool::new(true);
        let _ = reconcile_subtree(
            root_path,
            &crate::indexing::IndexPathSpace::root(),
            &h.conn,
            &h.writer,
            &pretripped,
        );
        h.writer.flush_blocking().unwrap();

        // Document (and pin) the interrupted state — THIS IS THE HAZARD: the interrupted pass listed
        // only the root, so the deleted `beta` row still lingers (an orphan), and its stale
        // listed_epoch drags the root's min_subtree_epoch below the current epoch (coverage looks
        // incomplete). The next complete pass must heal both, which the asserts below prove.
        assert!(
            resolve(&h, &root_path.join("beta")).is_some(),
            "interrupted reconcile that never reached the root's children leaves the deleted dir as a lingering row (the hazard)"
        );
        let current_after_first_bump = IndexStore::read_current_epoch(&h.conn).unwrap();
        assert!(
            dir_stats(&h, root_id).unwrap().min_subtree_epoch < current_after_first_bump,
            "the lingering stale child drags root coverage below the current epoch after the interrupted pass"
        );

        // Now a COMPLETE reconcile must sweep beta (it's a direct child of the re-listed root).
        bump_epoch(&h);
        reconcile_full(&h, root_path);

        assert!(
            resolve(&h, &root_path.join("beta")).is_none(),
            "deleted dir beta must be gone after a complete reconcile (no lingering orphan)"
        );
        assert_no_orphans(&h);
        let current = IndexStore::read_current_epoch(&h.conn).unwrap();
        assert_eq!(
            dir_stats(&h, root_id).unwrap().min_subtree_epoch,
            current,
            "root coverage restored to current epoch after complete reconcile"
        );

        h.writer.shutdown();
    }
}

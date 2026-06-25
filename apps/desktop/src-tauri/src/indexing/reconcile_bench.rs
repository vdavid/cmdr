//! M3.0 GATE — PERFORMANCE microbenchmark for the non-destructive reconcile rescan.
//!
//! THROWAWAY bench/measurement code for the M3.0 gate (docs/specs/2026-06-25-honest-index-sizes-plan.md,
//! Milestone 3). NOT a regression test, NOT production wiring. The lead reviews this, then it can be
//! deleted (or one no-op-cost assertion kept as the standing "reconcile-no-op writes zero rows" guard
//! the gate relied on). Marked `#[ignore]` so it never runs in CI; run explicitly:
//!
//!   find apps/desktop/src-tauri/src -name '*.rs' | xargs touch   # avoid stale COW build
//!   cargo nextest run -p cmdr-lib --no-capture reconcile_bench --run-ignored all
//!
//! ## What it measures
//!
//! The DB WRITE-PATH delta that the reconcile design introduces, isolated from FS/network walk cost.
//! The walk (read_dir / SMB list_directory) is unchanged from today's scan and dominated by I/O — that
//! is NOT what M3 changes, so we drive the writer with the exact message stream each strategy emits and
//! time the writer-side work. The three arms over one synthetic tree:
//!
//!   1. BASELINE (today): `TruncateData` → `InsertEntriesV2` batches → `ComputeAllAggregates`.
//!   2. RECONCILE NO-OP (nothing changed): for every dir, read DB children, diff against an identical
//!      "live listing", write nothing but `MarkDirsListed` + `PropagateMinSubtreeEpoch`. Steady-state
//!      rescan-of-unchanged-tree cost — the number the gate hinges on.
//!   3. RECONCILE 1%-CHANGED: same as (2) but ~1% of dirs get a child size-modify, so a bounded set of
//!      `UpsertEntryV2` writes + delta propagation fire.
//!
//! Arm (2) emits the SAME writer messages the real `reconcile_subtree` emits for an unchanged tree
//! (verified by reading reconciler.rs): on a no-change dir it sends nothing per-child (UpsertEntryV2 is
//! gated behind `changed`), only the post-walk `MarkDirsListed` + `PropagateMinSubtreeEpoch`. So this is
//! a faithful write-path model without paying the tempdir-build + read_dir cost a real walk would add
//! (which is identical across all three arms anyway, hence not the delta under test).

#[cfg(test)]
// Throwaway M3.0 perf bench: `eprintln!` is the deliverable (the measured numbers must be visible on
// `--nocapture`); `log::*` is level-filtered out under the test harness. This is `#[ignore]`d and never
// runs in CI, so the print_stderr ban (meant for production paths) is justifiably waived here only.
#[allow(
    clippy::print_stderr,
    reason = "throwaway #[ignore]d M3.0 perf bench; the measured numbers must print on --nocapture, and log::* is level-filtered under the test harness. Never runs in CI."
)]
mod bench {
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    use rusqlite::Connection;

    use crate::indexing::store::{EntryRow, IndexStore, ROOT_ID};
    use crate::indexing::stress_test_helpers::{build_synthetic_tree, setup_writer};
    use crate::indexing::writer::{IndexWriter, WriteMessage};

    /// Tree shape for the bench. `build_synthetic_tree(levels, dirs_per_level, files_per_dir, size)`
    /// branches `dirs_per_level` per parent each level, so dir count is
    /// `sum_{d=1..levels} dirs_per_level^d` and entries ≈ dirs * (1 + files_per_dir).
    ///
    /// (5, 8, 12, 4096) → 8+64+512+4096+32768 = 37,448 dirs, ~487k entries. Close to the local `root`
    /// entry scale; ~37k dirs lets us extrapolate dir-count-linear costs to the doc's ~538k-dir figure
    /// (×~14). Default runs in well under a minute. Bump LEVELS to 6 for a ~300k-dir / ~3.9M-entry run
    /// if you want to measure nearer full scale directly (minutes, lots of RAM).
    const LEVELS: usize = 5;
    const DIRS_PER_LEVEL: usize = 8;
    const FILES_PER_DIR: usize = 12;
    const FILE_SIZE: u64 = 4096;

    struct TreeFacts {
        entries: Vec<EntryRow>,
        dir_ids: Vec<i64>,
        n_entries: usize,
        n_dirs: usize,
    }

    fn build_tree() -> TreeFacts {
        let entries = build_synthetic_tree(LEVELS, DIRS_PER_LEVEL, FILES_PER_DIR, FILE_SIZE);
        let mut dir_ids: Vec<i64> = entries.iter().filter(|e| e.is_directory).map(|e| e.id).collect();
        dir_ids.push(ROOT_ID);
        let n_dirs = dir_ids.len();
        let n_entries = entries.len();
        TreeFacts {
            entries,
            dir_ids,
            n_entries,
            n_dirs,
        }
    }

    /// Populate a fresh DB to the "already fully indexed" state, then return a live writer + read conn.
    /// This is the precondition every reconcile arm starts from (a populated DB).
    fn populate(facts: &TreeFacts, listed_epoch: u64) -> (IndexWriter, Connection, tempfile::TempDir) {
        let (writer, read_conn, dir) = setup_writer();
        writer
            .send(WriteMessage::UpdateMeta {
                key: crate::indexing::store::CURRENT_EPOCH_KEY.to_string(),
                value: listed_epoch.to_string(),
            })
            .unwrap();
        for batch in facts.entries.chunks(2000) {
            writer.send(WriteMessage::InsertEntriesV2(batch.to_vec())).unwrap();
        }
        writer
            .send(WriteMessage::MarkDirsListed {
                ids: facts.dir_ids.clone(),
                epoch: listed_epoch,
            })
            .unwrap();
        writer.send(WriteMessage::ComputeAllAggregates).unwrap();
        writer.flush_blocking().unwrap();
        (writer, read_conn, dir)
    }

    /// Map dir_id -> its child EntryRows, from the in-memory tree (stand-in for the "live listing").
    fn children_by_parent(entries: &[EntryRow]) -> HashMap<i64, Vec<EntryRow>> {
        let mut m: HashMap<i64, Vec<EntryRow>> = HashMap::new();
        for e in entries {
            m.entry(e.parent_id).or_default().push(e.clone());
        }
        m
    }

    /// Drive the writer with the exact message stream a NO-OP reconcile emits: per dir, read DB
    /// children (the DB-side cost the real walk pays too) and diff against an identical live listing.
    /// Nothing changed ⇒ no per-child writes; only the post-walk marks + propagation.
    fn reconcile_noop_messages(writer: &IndexWriter, conn: &Connection, facts: &TreeFacts, epoch: u64) {
        // Phase A: per-dir DB read + name-diff (the DB-side cost of the walk).
        let ta = Instant::now();
        for &dir_id in &facts.dir_ids {
            let db_children = IndexStore::list_children_on(dir_id, conn).unwrap();
            let mut by_name: HashMap<String, &EntryRow> = HashMap::with_capacity(db_children.len());
            for row in &db_children {
                by_name.insert(crate::indexing::store::normalize_for_comparison(&row.name), row);
            }
            std::hint::black_box(&by_name);
        }
        let diff_ms = ms(ta.elapsed());

        // Phase B: MarkDirsListed (PK UPDATE, batched).
        let tb = Instant::now();
        for chunk in facts.dir_ids.chunks(900) {
            writer
                .send(WriteMessage::MarkDirsListed {
                    ids: chunk.to_vec(),
                    epoch,
                })
                .unwrap();
        }
        writer.flush_blocking().unwrap();
        let mark_ms = ms(tb.elapsed());

        // Phase C: PropagateMinSubtreeEpoch per dir (the suspected dominant cost).
        let tc = Instant::now();
        for &dir_id in facts.dir_ids.iter().rev() {
            writer.send(WriteMessage::PropagateMinSubtreeEpoch(dir_id)).unwrap();
        }
        writer.flush_blocking().unwrap();
        let prop_ms = ms(tc.elapsed());

        eprintln!("  no-op phase breakdown: diff(read)={diff_ms}ms  mark={mark_ms}ms  propagate={prop_ms}ms");
    }

    /// Drive the writer for a reconcile where ~1% of dirs each get one change (a child file size bump),
    /// plus the marks + propagation. Models the steady-state "a little changed" rescan.
    fn reconcile_one_percent_messages(writer: &IndexWriter, conn: &Connection, facts: &TreeFacts, epoch: u64) -> usize {
        let by_parent = children_by_parent(&facts.entries);
        let mut changed = 0usize;
        let change_every = 100; // 1%
        for (i, &dir_id) in facts.dir_ids.iter().enumerate() {
            let db_children = IndexStore::list_children_on(dir_id, conn).unwrap();
            let mut by_name: HashMap<String, &EntryRow> = HashMap::with_capacity(db_children.len());
            for row in &db_children {
                by_name.insert(crate::indexing::store::normalize_for_comparison(&row.name), row);
            }
            std::hint::black_box(&by_name);

            if i % change_every == 0
                && let Some(kids) = by_parent.get(&dir_id)
                && let Some(file) = kids.iter().find(|k| !k.is_directory)
            {
                writer
                    .send(WriteMessage::UpsertEntryV2 {
                        parent_id: dir_id,
                        name: file.name.clone(),
                        is_directory: false,
                        is_symlink: false,
                        logical_size: Some(FILE_SIZE + 1),
                        physical_size: Some(FILE_SIZE + 1),
                        modified_at: Some(1_700_000_001),
                        inode: None,
                        nlink: None,
                    })
                    .unwrap();
                changed += 1;
            }
        }
        for chunk in facts.dir_ids.chunks(900) {
            writer
                .send(WriteMessage::MarkDirsListed {
                    ids: chunk.to_vec(),
                    epoch,
                })
                .unwrap();
        }
        for &dir_id in facts.dir_ids.iter().rev() {
            writer.send(WriteMessage::PropagateMinSubtreeEpoch(dir_id)).unwrap();
        }
        writer.flush_blocking().unwrap();
        changed
    }

    /// ALTERNATIVE no-op reconcile coverage-refresh: instead of 37k per-dir `PropagateMinSubtreeEpoch`
    /// ancestor-walks, mark all dirs then run ONE bottom-up aggregate (`ComputeAllAggregates`, which
    /// with empty accumulator maps falls back to the O(dirs) SQL `compute_all_aggregates_reported`).
    /// This is the design the report recommends for the full-rescan entry point: stamp + single
    /// bottom-up min recompute, not per-dir propagation. Measures the same diff-read phase + a single
    /// aggregate.
    fn reconcile_noop_single_aggregate(writer: &IndexWriter, conn: &Connection, facts: &TreeFacts, epoch: u64) {
        let ta = Instant::now();
        for &dir_id in &facts.dir_ids {
            let db_children = IndexStore::list_children_on(dir_id, conn).unwrap();
            let mut by_name: HashMap<String, &EntryRow> = HashMap::with_capacity(db_children.len());
            for row in &db_children {
                by_name.insert(crate::indexing::store::normalize_for_comparison(&row.name), row);
            }
            std::hint::black_box(&by_name);
        }
        let diff_ms = ms(ta.elapsed());

        let tb = Instant::now();
        for chunk in facts.dir_ids.chunks(900) {
            writer
                .send(WriteMessage::MarkDirsListed {
                    ids: chunk.to_vec(),
                    epoch,
                })
                .unwrap();
        }
        // Single bottom-up recompute (sizes unchanged but coverage re-mins in one O(dirs) pass).
        writer.send(WriteMessage::ComputeAllAggregates).unwrap();
        writer.flush_blocking().unwrap();
        let mark_and_agg_ms = ms(tb.elapsed());
        eprintln!("  no-op(single-aggregate) breakdown: diff(read)={diff_ms}ms  mark+aggregate={mark_and_agg_ms}ms");
    }

    /// Baseline: today's truncate + bulk reinsert + full aggregate, on a populated DB.
    fn truncate_rebuild_messages(writer: &IndexWriter, facts: &TreeFacts, epoch: u64) {
        writer.send(WriteMessage::TruncateData).unwrap();
        for batch in facts.entries.chunks(2000) {
            writer.send(WriteMessage::InsertEntriesV2(batch.to_vec())).unwrap();
        }
        writer
            .send(WriteMessage::MarkDirsListed {
                ids: facts.dir_ids.clone(),
                epoch,
            })
            .unwrap();
        writer.send(WriteMessage::ComputeAllAggregates).unwrap();
        writer.flush_blocking().unwrap();
    }

    fn ms(d: Duration) -> u128 {
        d.as_millis()
    }

    #[test]
    #[ignore = "M3.0 perf bench, run explicitly with --run-ignored all"]
    fn m3_reconcile_perf_gate() {
        let facts = build_tree();
        eprintln!(
            "\n=== M3.0 RECONCILE PERF GATE ===\nTree: levels={LEVELS} dirs/level={DIRS_PER_LEVEL} files/dir={FILES_PER_DIR}\n      {} entries, {} dirs\n",
            facts.n_entries, facts.n_dirs
        );

        // ── Arm 1: BASELINE (truncate + bulk reinsert + aggregate), on a populated DB ──
        let baseline_ms = {
            let (writer, _rc, _dir) = populate(&facts, 5);
            let t = Instant::now();
            truncate_rebuild_messages(&writer, &facts, 6);
            let e = t.elapsed();
            writer.shutdown();
            ms(e)
        };

        // ── Arm 2: RECONCILE NO-OP (nothing changed) ──
        let (noop_ms, noop_rows_after) = {
            let (writer, read_conn, _dir) = populate(&facts, 5);
            let entries_before: i64 = read_conn
                .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
                .unwrap();
            let t = Instant::now();
            reconcile_noop_messages(&writer, &read_conn, &facts, 6);
            let e = t.elapsed();
            let entries_after: i64 = read_conn
                .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
                .unwrap();
            assert_eq!(
                entries_before, entries_after,
                "no-op reconcile must not change entry row count (before={entries_before}, after={entries_after})"
            );
            writer.shutdown();
            (ms(e), entries_after)
        };

        // ── Arm 2b: RECONCILE NO-OP via single bottom-up aggregate (recommended design) ──
        let noop_agg_ms = {
            let (writer, read_conn, _dir) = populate(&facts, 5);
            let t = Instant::now();
            reconcile_noop_single_aggregate(&writer, &read_conn, &facts, 6);
            let e = t.elapsed();
            // Correctness: after the single aggregate, every dir's min_subtree_epoch must be the new
            // epoch (6) — i.e. the cheaper design produces the right honest-coverage result.
            let root_epoch = IndexStore::get_dir_stats_by_id(&read_conn, ROOT_ID)
                .unwrap()
                .map(|s| s.min_subtree_epoch)
                .unwrap_or(0);
            assert_eq!(
                root_epoch, 6,
                "single-aggregate no-op reconcile must re-stamp coverage to the new epoch (got {root_epoch})"
            );
            writer.shutdown();
            ms(e)
        };

        // ── Arm 3: RECONCILE 1%-CHANGED ──
        let (one_pct_ms, n_changed) = {
            let (writer, read_conn, _dir) = populate(&facts, 5);
            let t = Instant::now();
            let changed = reconcile_one_percent_messages(&writer, &read_conn, &facts, 6);
            let e = t.elapsed();
            writer.shutdown();
            (ms(e), changed)
        };

        eprintln!("RESULTS (writer-side, ms):");
        eprintln!("  baseline (truncate+reinsert+aggregate): {baseline_ms} ms");
        eprintln!("  reconcile NO-OP (per-dir propagate):      {noop_ms} ms   (entries still {noop_rows_after})");
        eprintln!("  reconcile NO-OP (single aggregate):       {noop_agg_ms} ms   (recommended design)");
        eprintln!("  reconcile 1%-changed ({n_changed} dirs):           {one_pct_ms} ms");
        let ratio = if noop_ms == 0 {
            f64::INFINITY
        } else {
            baseline_ms as f64 / noop_ms as f64
        };
        eprintln!("  no-op speedup vs baseline: {ratio:.1}x");
        eprintln!("=== END PERF GATE ===\n");
    }
}

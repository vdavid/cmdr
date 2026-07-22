//! What the cost budget does to the reconcile WALK: which directories still get
//! read, and — the data-safety half — what must NOT happen to the rows and epochs
//! of the ones that don't. The pure decision is tested in `cost_budget.rs`.

use super::*;

/// A reader over the real filesystem that spends `slow` on every path containing
/// `marker`. The deterministic stand-in for a pathological subtree: the walk sees
/// real children and real diffs, but one branch costs measurable time.
fn scripted_reader(marker: &'static str, slow: Duration) -> GuardedReader {
    let space = IndexPathSpace::root();
    GuardedReader::with_read_fn(
        Duration::from_secs(5),
        Arc::new(move |p: &Path| {
            if p.to_string_lossy().contains(marker) {
                std::thread::sleep(slow);
            }
            reconciler::read_fs_children(p, &space)
        }),
    )
}

/// Anchors one level below the walk root. A read counts as slow past 50 ms (plus
/// 1 ms per entry), and one slow read is enough to condemn a subtree whose reads
/// are more than half slow and which has wasted more than 150 ms: `pricey`
/// (300 ms for a one-entry directory) blows it on its own first read, while
/// `cheap`'s tiny tempdir reads stay orders of magnitude below the slow line even
/// on a loaded machine.
fn tiny_budget() -> CostBudget {
    CostBudget {
        anchor_depth: 1,
        fixed_allowance: Duration::from_millis(50),
        per_entry_allowance: Duration::from_millis(1),
        max_slow_fraction: 0.5,
        min_slow_reads: 1,
        min_slow_time_wasted: Duration::from_millis(150),
    }
}

/// Build `cheap/a/b/leaf.txt` + `pricey/deep/deeper/keep.txt` and index it all at
/// epoch 1, then bump to epoch 2. Returns the tree root.
fn budget_tree(h: &Harness) -> tempfile::TempDir {
    let root = tree_root();
    let rp = root.path();
    ensure_path_in_db(h, &norm(rp));
    std::fs::create_dir_all(rp.join("cheap/a/b")).unwrap();
    std::fs::write(rp.join("cheap/a/b/leaf.txt"), b"x").unwrap();
    std::fs::create_dir_all(rp.join("pricey/deep/deeper")).unwrap();
    std::fs::write(rp.join("pricey/deep/deeper/keep.txt"), b"keep").unwrap();
    run_reconcile(h, rp, false).expect("first reconcile");
    bump_epoch(h);
    root
}

/// Every directory in a subtree that stays inside its budget is still walked —
/// the backstop must not cost the healthy 98% of a volume anything.
#[test]
fn a_subtree_within_the_read_budget_is_fully_walked() {
    let h = setup();
    let root = budget_tree(&h);
    let rp = root.path();

    let tools = WalkTools {
        reader: scripted_reader("pricey", Duration::from_millis(300)),
        budget: tiny_budget(),
    };
    run_reconcile_with(&h, rp, tools, false).expect("reconcile");

    for rel in ["cheap", "cheap/a", "cheap/a/b"] {
        let id = resolve(&h, &rp.join(rel)).expect("indexed");
        assert_eq!(listed_epoch(&h, id), Some(2), "{rel} re-listed at the new epoch");
    }
}

/// A subtree that spends more read time than its budget stops being descended,
/// while the rest of the walk carries on.
#[test]
fn a_subtree_over_the_read_budget_stops_being_descended() {
    let h = setup();
    let root = budget_tree(&h);
    let rp = root.path();
    let before_trips = DEBUG_STATS.reconcile_budget_subtrees.load(Ordering::Relaxed);

    let tools = WalkTools {
        reader: scripted_reader("pricey", Duration::from_millis(300)),
        budget: tiny_budget(),
    };
    run_reconcile_with(&h, rp, tools, false).expect("reconcile");

    let pricey = resolve(&h, &rp.join("pricey")).expect("indexed");
    let deep = resolve(&h, &rp.join("pricey/deep")).expect("indexed");
    assert_eq!(
        listed_epoch(&h, pricey),
        Some(2),
        "the anchor itself is read — it's the read that spends the budget"
    );
    assert_eq!(
        listed_epoch(&h, deep),
        Some(1),
        "its children are not descended once the budget is gone"
    );
    let cheap = resolve(&h, &rp.join("cheap/a/b")).expect("indexed");
    assert_eq!(listed_epoch(&h, cheap), Some(2), "the rest of the walk carries on");
    assert!(
        DEBUG_STATS.reconcile_budget_subtrees.load(Ordering::Relaxed) > before_trips,
        "the activation is counted on the debug surface"
    );
}

/// Data safety: a skipped subtree is "we never listed it", NEVER "we listed it
/// and it was empty". Handing `diff_dir_against_db` an empty listing would reap
/// every DB child and strip those bytes out of every ancestor's `dir_stats` with
/// no way back.
#[test]
fn a_budget_skipped_subtree_keeps_every_row_and_its_sizes() {
    let h = setup();
    let root = budget_tree(&h);
    let rp = root.path();
    let deep = resolve(&h, &rp.join("pricey/deep")).expect("indexed");
    let stats_before = dir_stats(&h, deep).expect("deep has stats");

    let tools = WalkTools {
        reader: scripted_reader("pricey", Duration::from_millis(300)),
        budget: tiny_budget(),
    };
    run_reconcile_with(&h, rp, tools, false).expect("reconcile");

    assert!(
        resolve(&h, &rp.join("pricey/deep/deeper")).is_some(),
        "a skipped subtree's directories stay in the index"
    );
    assert!(
        resolve(&h, &rp.join("pricey/deep/deeper/keep.txt")).is_some(),
        "and so do their files"
    );
    let stats_after = dir_stats(&h, deep).expect("deep still has stats");
    assert_eq!(
        stats_after.recursive_file_count, stats_before.recursive_file_count,
        "the skipped subtree's file count is untouched"
    );
    assert_eq!(
        stats_after.recursive_physical_size, stats_before.recursive_physical_size,
        "and so are its bytes — they must not leave the ancestors' ledger"
    );
}

/// Data safety: a skipped directory keeps its OLD `listed_epoch`, and nothing in
/// the tree gets stamped `0`. A zero absorbs up the whole ancestor chain
/// (`absorbing_min_epoch`), which would mark `~` and `/` incomplete and make
/// `expected_totals` return `None` for every copy of the home folder.
#[test]
fn a_budget_skipped_subtree_leaves_its_epoch_and_every_ancestor_epoch_untouched() {
    let h = setup();
    let root = budget_tree(&h);
    let rp = root.path();

    // Every ancestor ABOVE the walk root, up to `/`, as it stands before the walk.
    // (The walk root itself is listed, so it legitimately moves to the new epoch.)
    let mut ancestor_epochs: Vec<(PathBuf, Option<u64>)> = Vec::new();
    let mut ancestor = rp.parent().map(Path::to_path_buf);
    while let Some(path) = ancestor {
        let id = resolve(&h, &path).expect("ancestor indexed");
        ancestor_epochs.push((path.clone(), listed_epoch(&h, id)));
        ancestor = path.parent().map(Path::to_path_buf);
    }
    let zeroes_before = zero_epoch_count(&h);

    let tools = WalkTools {
        reader: scripted_reader("pricey", Duration::from_millis(300)),
        budget: tiny_budget(),
    };
    run_reconcile_with(&h, rp, tools, false).expect("reconcile");

    for rel in ["pricey/deep", "pricey/deep/deeper"] {
        let id = resolve(&h, &rp.join(rel)).expect("indexed");
        assert_eq!(
            listed_epoch(&h, id),
            Some(1),
            "{rel} must keep the epoch it was last listed at (not 0, not the new one)"
        );
    }
    for (ancestor, before) in ancestor_epochs {
        let id = resolve(&h, &ancestor).expect("ancestor still indexed");
        assert_eq!(
            listed_epoch(&h, id),
            before,
            "the skip must leave every ancestor epoch exactly as it was ({})",
            ancestor.display()
        );
    }
    assert_eq!(
        zero_epoch_count(&h),
        zeroes_before,
        "a budget skip must never stamp a zero epoch on anything"
    );
}

/// How many entries carry `listed_epoch = 0` ("never listed"). A zero absorbs up
/// the ancestor chain, so the reconcile must never mint a new one.
fn zero_epoch_count(h: &Harness) -> i64 {
    conn(h)
        .query_row("SELECT COUNT(*) FROM entries WHERE listed_epoch = 0", [], |r| r.get(0))
        .unwrap()
}

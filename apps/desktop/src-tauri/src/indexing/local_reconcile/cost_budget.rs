//! The cost backstop for the serial local reconcile walk: a slow-read budget per
//! subtree, so one pathological corner of a volume can't eat the whole walk.
//!
//! Pure and threshold-injected (the `event_loop/verify_guard.rs` /
//! `reconciler/rescan_route.rs` shape), so the boundary is pinned in the ms-scale
//! unit tier and the walk test can drive it with a 150 ms budget instead of a
//! 20-minute fixture.
//!
//! ## What it measures
//!
//! Read LATENCY, never cumulative read time. A big healthy directory tree and a
//! pathological one both spend a lot of wall clock; only the pathological one
//! spends it per unit of work. So each read gets an allowance proportional to what
//! it returned ([`SLOW_READ_FIXED_ALLOWANCE`] plus
//! [`SLOW_READ_PER_ENTRY_ALLOWANCE`] per entry), and only the time of reads that
//! blow past their allowance is charged to a budget. A subtree of a million fast
//! reads charges nothing however large it grows.
//!
//! ## What it charges, and to whom
//!
//! Every SLOW directory read is charged to ONE accumulator: the walked directory's
//! ancestor at [`ANCHOR_DEPTH`] below the volume root (its *anchor*). Once an
//! anchor has spent more than the budget across at least [`MIN_SLOW_READS`] slow
//! reads, the walk stops descending into that subtree and carries on everywhere
//! else. Nothing above the anchor depth carries a budget, so the top of the tree is
//! always walked.
//!
//! Charging each read up its WHOLE ancestor chain instead would decide the same
//! thing more expensively: a node's accumulated cost is always ≥ any
//! descendant's, so with one flat budget the shallowest budgeted ancestor always
//! trips first, and pruning there prunes everything below it anyway. One
//! accumulator per anchor is that same verdict at O(1) per read.
//!
//! ❌ Skipping means "don't descend", NEVER "listed and found nothing". See
//! `indexing/DETAILS.md` § "The reconcile cost budget" for the two data-safety
//! rules the caller must honour.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

/// What one directory read may cost before it counts as pathologically slow,
/// regardless of how many entries it returned. Covers the fixed part of a read:
/// the syscall, the directory open, one `lstat`-ish round trip.
///
/// **Provisional.** An ordinary boot-volume read measured 0.56 ms mean over
/// 579,107 reads (verified on macOS 15, `CMDR_RECONCILE_LATENCY_SPIKE` walk,
/// 2026-07-21, see
/// [`docs/notes/indexing-benchmarks-2026-07-21.md`](../../../../../../docs/notes/indexing-benchmarks-2026-07-21.md)),
/// so 20 ms sits ~35× above it. The pathology it separates from is three orders of
/// magnitude away (a File Provider phone's reads ran 0.7–4.4 s EACH), so the exact
/// number is not load-bearing; the generous side is the safe side.
pub(super) const SLOW_READ_FIXED_ALLOWANCE: Duration = Duration::from_millis(20);

/// What one directory read may additionally cost per entry it returned. This is
/// the term that keeps a BIG directory from looking pathological: reading 200,000
/// entries is legitimately slower than reading 10.
///
/// **Provisional.** The same walk measured ~58 µs per entry all-in on ordinary
/// directories (0.56 ms over ~9.6 entries mean) and ~20 µs per entry on the
/// 200,000-file test fixture that took 3.9 s to read, so 100 µs per entry leaves
/// both a 2–5× margin while staying far under the pathological case.
pub(super) const SLOW_READ_PER_ENTRY_ALLOWANCE: Duration = Duration::from_micros(100);

/// Wall-clock time in SLOW reads one anchor subtree may spend before the walk
/// stops descending into it. Fast reads never count towards it.
///
/// **Provisional.** It rests on one measured reconcile of one boot volume (see the
/// benchmark note linked above): the whole 477 s walk spent 40.9 s in File Provider
/// reads in total, so 10 s of *pathological* time inside ONE anchor subtree is
/// already a large share of the walk's worst quarter. Ordinary subtrees never
/// approach it: at 20 ms a read, a healthy subtree would need 500 reads that each
/// blew their allowance.
///
/// The activation counters (`reconcileBudgetSubtrees` /
/// `reconcileBudgetSkippedDirs` on the debug surface) are the instrument for
/// retuning it: if real machines trip subtrees that should have been walked, this
/// number moves, not the logic.
pub(super) const SUBTREE_SLOW_READ_BUDGET: Duration = Duration::from_secs(10);

/// How many slow reads an anchor subtree must have seen before it can be refused
/// at all.
///
/// **Provisional**, and a judgement rather than a measurement: one or two
/// pathological directories are a local problem, not a verdict on their whole
/// subtree, and refusing a subtree costs every innocent directory under it. Three
/// says "this is systemic, not a hiccup" while still tripping the measured phone
/// (thousands of slow reads) many times over. It also makes the floor independent
/// of `scanner::LOCAL_LIST_TIMEOUT`: without it, two hung reads alone could condemn
/// a subtree.
pub(super) const MIN_SLOW_READS: u32 = 3;

/// Depth below the volume root at which budget anchors sit.
///
/// **Provisional**, and a granularity choice rather than a measured one: an
/// anchor is the unit we refuse as a whole, so shallower anchors cover more of
/// the volume while taking more innocent directories down with a trip. Five puts
/// the anchor at app/project granularity on a boot volume (`~/Library/Caches/
/// go-build`, `~/Library/Application Support/Slack`, `~/projects-git/vdavid/cmdr`),
/// which is where the measured offenders sit.
pub(super) const ANCHOR_DEPTH: usize = 5;

/// What one directory read cost, and how much work it did for that cost. Both
/// halves are needed: time alone can't tell a slow filesystem from a big directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ReadCost {
    pub(super) duration: Duration,
    /// Entries the read returned. An unlistable or timed-out read returns none, so
    /// it is measured against the fixed allowance alone. That is exactly right: a
    /// read that hung for 15 s and produced nothing is the pathology itself.
    pub(super) entries: usize,
}

/// What the walk should do with a directory it just popped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BudgetVerdict {
    /// Within budget: read it and diff it.
    Walk,
    /// Its subtree is over budget: don't descend. ❌ Not "listed and found
    /// nothing" — leave its rows and its `listed_epoch` alone.
    Skip,
}

/// One subtree's shared slow-read accumulator.
struct Anchor {
    path: PathBuf,
    /// Time spent in reads that blew their allowance. Fast reads add nothing.
    slow_spent: Cell<Duration>,
    slow_reads: Cell<u32>,
    /// Set the first time the subtree trips, so one trip is reported once however
    /// many directories it later refuses.
    reported: Cell<bool>,
}

/// A directory's place in the budget: how deep it sits, and which subtree pays
/// for it. Cheap to clone (one `Rc` bump), and carried alongside every queued
/// directory.
#[derive(Clone)]
pub(super) struct Anchorage {
    depth: usize,
    anchor: Option<Rc<Anchor>>,
}

/// A subtree that has just crossed its budget, for logging and counting.
pub(super) struct TrippedSubtree {
    pub(super) path: PathBuf,
    pub(super) slow_spent: Duration,
    pub(super) slow_reads: u32,
}

/// The budget policy: where anchors sit, what makes a read slow, and how much slow
/// reading one subtree may pay for.
pub(super) struct CostBudget {
    anchor_depth: usize,
    fixed_allowance: Duration,
    per_entry_allowance: Duration,
    per_subtree: Duration,
    min_slow_reads: u32,
}

impl CostBudget {
    /// The shipped policy, from the constants above.
    pub(super) fn production() -> Self {
        Self::new(
            ANCHOR_DEPTH,
            SLOW_READ_FIXED_ALLOWANCE,
            SLOW_READ_PER_ENTRY_ALLOWANCE,
            SUBTREE_SLOW_READ_BUDGET,
            MIN_SLOW_READS,
        )
    }

    pub(super) fn new(
        anchor_depth: usize,
        fixed_allowance: Duration,
        per_entry_allowance: Duration,
        per_subtree: Duration,
        min_slow_reads: u32,
    ) -> Self {
        Self {
            anchor_depth,
            fixed_allowance,
            per_entry_allowance,
            per_subtree,
            min_slow_reads,
        }
    }

    /// The anchorage of the walk's root directory.
    pub(super) fn root_anchorage(&self, root: &Path) -> Anchorage {
        self.anchorage_at(0, root, None)
    }

    /// The anchorage of a child of `parent`: one level deeper, anchored to the
    /// same subtree unless this is the anchor depth itself.
    pub(super) fn child(&self, parent: &Anchorage, child_path: &Path) -> Anchorage {
        self.anchorage_at(parent.depth + 1, child_path, parent.anchor.clone())
    }

    fn anchorage_at(&self, depth: usize, path: &Path, inherited: Option<Rc<Anchor>>) -> Anchorage {
        let anchor = if depth == self.anchor_depth {
            Some(Rc::new(Anchor {
                path: path.to_path_buf(),
                slow_spent: Cell::new(Duration::ZERO),
                slow_reads: Cell::new(0),
                reported: Cell::new(false),
            }))
        } else {
            inherited
        };
        Anchorage { depth, anchor }
    }

    /// Whether this directory may still be read. A directory above the anchor
    /// depth carries no anchor and is always walked.
    pub(super) fn verdict(&self, at: &Anchorage) -> BudgetVerdict {
        match &at.anchor {
            Some(anchor) if self.over(anchor) => BudgetVerdict::Skip,
            _ => BudgetVerdict::Walk,
        }
    }

    /// Charge one directory read to this directory's subtree. A read that stayed
    /// within its allowance costs the subtree nothing. Returns the subtree on the
    /// read that pushes it over, and only on that read.
    pub(super) fn charge(&self, at: &Anchorage, read: ReadCost) -> Option<TrippedSubtree> {
        let anchor = at.anchor.as_ref()?;
        if !self.is_slow(&read) {
            return None;
        }
        anchor
            .slow_spent
            .set(anchor.slow_spent.get().saturating_add(read.duration));
        anchor.slow_reads.set(anchor.slow_reads.get().saturating_add(1));
        if !self.over(anchor) || anchor.reported.get() {
            return None;
        }
        anchor.reported.set(true);
        Some(TrippedSubtree {
            path: anchor.path.clone(),
            slow_spent: anchor.slow_spent.get(),
            slow_reads: anchor.slow_reads.get(),
        })
    }

    /// Whether a read did so little for its time that it counts as pathological.
    fn is_slow(&self, read: &ReadCost) -> bool {
        read.duration > self.allowance_for(read.entries)
    }

    /// What a read returning `entries` entries is allowed to cost.
    fn allowance_for(&self, entries: usize) -> Duration {
        let entries = u32::try_from(entries).unwrap_or(u32::MAX);
        self.fixed_allowance
            .saturating_add(self.per_entry_allowance.saturating_mul(entries))
    }

    /// A subtree that has spent EXACTLY its budget has honoured it and finishes;
    /// only spending more than it stops the descent. The sample floor is a
    /// precondition, not a tiebreak: too few slow reads is no verdict at all.
    fn over(&self, anchor: &Anchor) -> bool {
        anchor.slow_reads.get() >= self.min_slow_reads && anchor.slow_spent.get() > self.per_subtree
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Anchors one level down. A read is slow past 10 ms + 1 ms per entry, and a
    /// subtree may spend 100 ms across at least two slow reads.
    fn budget() -> CostBudget {
        CostBudget::new(
            1,
            Duration::from_millis(10),
            Duration::from_millis(1),
            Duration::from_millis(100),
            2,
        )
    }

    /// A read that took `ms` and returned `entries` entries.
    fn read(ms: u64, entries: usize) -> ReadCost {
        ReadCost {
            duration: Duration::from_millis(ms),
            entries,
        }
    }

    /// Descend from `/vol` through `names`, returning each level's anchorage.
    fn descend(b: &CostBudget, names: &[&str]) -> Vec<Anchorage> {
        let mut path = PathBuf::from("/vol");
        let mut at = b.root_anchorage(&path);
        let mut out = vec![at.clone()];
        for name in names {
            path.push(name);
            at = b.child(&at, &path);
            out.push(at.clone());
        }
        out
    }

    /// The false positive this metric exists to kill: David's repo anchor is slow
    /// only because it is enormous, and it must stay walked however much it grows.
    #[test]
    fn a_subtree_of_many_fast_reads_is_never_refused_however_large_it_gets() {
        let b = budget();
        let chain = descend(&b, &["big-healthy-repo", "deep"]);
        for _ in 0..100_000 {
            // 5 ms for 20 entries: comfortably inside the 30 ms allowance.
            assert!(b.charge(&chain[1], read(5, 20)).is_none());
        }
        assert_eq!(b.verdict(&chain[2]), BudgetVerdict::Walk);
    }

    /// The same, for the shape cumulative time got most wrong: one huge directory
    /// whose read takes seconds because it returned a quarter of a million entries.
    #[test]
    fn a_huge_directory_is_not_slow_when_its_time_matches_its_entries() {
        let b = budget();
        let chain = descend(&b, &["fixtures", "deep"]);
        for _ in 0..10 {
            // 200 ms for 200,000 entries: 1 µs each, far under the 1 ms allowance.
            assert!(b.charge(&chain[1], read(200, 200_000)).is_none());
        }
        assert_eq!(b.verdict(&chain[2]), BudgetVerdict::Walk);
    }

    /// The phone: a handful of reads that each take an eternity for a few entries.
    #[test]
    fn a_subtree_of_a_few_slow_reads_is_refused() {
        let b = budget();
        let chain = descend(&b, &["phone", "deep"]);
        for _ in 0..3 {
            // 50 ms for 3 entries, against a 13 ms allowance.
            b.charge(&chain[1], read(50, 3));
        }
        assert_eq!(b.verdict(&chain[2]), BudgetVerdict::Skip);
    }

    /// Two slow reads must not condemn a subtree, however slow they are: one dead
    /// directory is a local problem, not a verdict on everything around it.
    #[test]
    fn too_few_slow_reads_never_trip_the_budget() {
        let b = CostBudget::new(
            1,
            Duration::from_millis(10),
            Duration::from_millis(1),
            Duration::from_millis(100),
            3,
        );
        let chain = descend(&b, &["one-dead-dir", "deep"]);
        assert!(b.charge(&chain[1], read(15_000, 0)).is_none());
        assert!(b.charge(&chain[1], read(15_000, 0)).is_none());
        assert_eq!(
            b.verdict(&chain[2]),
            BudgetVerdict::Walk,
            "30 s in two reads is under the sample floor"
        );
        assert!(
            b.charge(&chain[1], read(15_000, 0)).is_some(),
            "the third slow read is what makes it systemic"
        );
        assert_eq!(b.verdict(&chain[2]), BudgetVerdict::Skip);
    }

    #[test]
    fn a_subtree_under_its_budget_keeps_being_walked() {
        let b = budget();
        let chain = descend(&b, &["cheap", "deep"]);
        assert!(b.charge(&chain[1], read(30, 0)).is_none());
        assert!(b.charge(&chain[1], read(30, 0)).is_none());
        assert_eq!(b.verdict(&chain[2]), BudgetVerdict::Walk);
    }

    #[test]
    fn spending_exactly_the_budget_still_finishes() {
        let b = budget();
        let chain = descend(&b, &["exact", "deep"]);
        assert!(b.charge(&chain[1], read(50, 0)).is_none());
        assert!(b.charge(&chain[1], read(50, 0)).is_none());
        assert_eq!(b.verdict(&chain[2]), BudgetVerdict::Walk);
    }

    #[test]
    fn an_over_budget_subtree_does_not_touch_its_siblings() {
        let b = budget();
        let pricey = descend(&b, &["pricey", "deep"]);
        let cheap = descend(&b, &["cheap", "deep"]);
        for _ in 0..3 {
            b.charge(&pricey[1], read(500, 0));
        }
        assert_eq!(b.verdict(&pricey[2]), BudgetVerdict::Skip);
        assert_eq!(
            b.verdict(&cheap[2]),
            BudgetVerdict::Walk,
            "a sibling subtree is unaffected"
        );
    }

    #[test]
    fn everything_above_the_anchor_depth_is_always_walked() {
        let b = CostBudget::new(
            2,
            Duration::from_millis(10),
            Duration::from_millis(1),
            Duration::from_millis(100),
            1,
        );
        let chain = descend(&b, &["a", "b"]);
        // Charging the root and the depth-1 dir is a no-op: they carry no anchor.
        assert!(b.charge(&chain[0], read(60_000, 0)).is_none());
        assert!(b.charge(&chain[1], read(60_000, 0)).is_none());
        assert_eq!(b.verdict(&chain[0]), BudgetVerdict::Walk);
        assert_eq!(b.verdict(&chain[1]), BudgetVerdict::Walk);
    }

    #[test]
    fn every_descendant_charges_the_same_anchor() {
        let b = budget();
        let chain = descend(&b, &["pricey", "deep", "deeper"]);
        // Three slow reads, each well under the budget alone, over it together.
        for level in &chain[1..] {
            b.charge(level, read(40, 0));
        }
        assert_eq!(b.verdict(&chain[1]), BudgetVerdict::Skip, "the anchor itself is over");
        assert_eq!(b.verdict(&chain[3]), BudgetVerdict::Skip, "and so is its deepest child");
    }

    #[test]
    fn a_trip_is_reported_once_with_the_subtree_and_what_it_spent() {
        let b = budget();
        let chain = descend(&b, &["pricey", "deep"]);
        assert!(b.charge(&chain[1], read(60, 0)).is_none());
        let tripped = b
            .charge(&chain[2], read(60, 0))
            .expect("the read that crosses the budget reports the subtree");
        assert_eq!(tripped.path, PathBuf::from("/vol/pricey"));
        assert_eq!(tripped.slow_spent, Duration::from_millis(120));
        assert_eq!(tripped.slow_reads, 2);
        assert!(
            b.charge(&chain[2], read(60, 0)).is_none(),
            "later reads in an already-tripped subtree report nothing"
        );
    }

    /// Fast reads inside an otherwise pathological subtree don't hurry the trip:
    /// the budget only ever counts the time the subtree actually wasted.
    #[test]
    fn fast_reads_never_count_towards_the_budget() {
        let b = budget();
        let chain = descend(&b, &["mixed", "deep"]);
        for _ in 0..10_000 {
            b.charge(&chain[1], read(1, 5));
        }
        assert_eq!(b.verdict(&chain[2]), BudgetVerdict::Walk);
        let tripped = {
            b.charge(&chain[1], read(60, 0));
            b.charge(&chain[1], read(60, 0))
                .expect("two slow reads blow the budget")
        };
        assert_eq!(
            tripped.slow_spent,
            Duration::from_millis(120),
            "only the slow reads' time is on the ledger"
        );
    }
}

//! The cost backstop for the serial local reconcile walk: a read-time budget per
//! subtree, so one pathological corner of a volume can't eat the whole walk.
//!
//! Pure and threshold-injected (the `event_loop/verify_guard.rs` /
//! `reconciler/rescan_route.rs` shape), so the boundary is pinned in the ms-scale
//! unit tier and the walk test can drive it with a 40 ms budget instead of a
//! 20-minute fixture.
//!
//! ## What it charges, and to whom
//!
//! Every directory read is charged to ONE accumulator: the walked directory's
//! ancestor at [`ANCHOR_DEPTH`] below the volume root (its *anchor*). Once an
//! anchor has spent more than the budget, the walk stops descending into that
//! subtree and carries on everywhere else. Nothing above the anchor depth carries
//! a budget, so the top of the tree is always walked.
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

/// Wall-clock directory-read time one anchor subtree may spend before the walk
/// stops descending into it.
///
/// **Provisional.** It rests on a single measured reconcile of one boot volume
/// (verified on macOS 15, `CMDR_RECONCILE_LATENCY_SPIKE` walk, 2026-07-20, see
/// [`docs/notes/reconcile-latency-spike.md`](../../../../../../docs/notes/reconcile-latency-spike.md)):
/// 1,309 s over 593,436 directories, 92.3% of it inside `read_dir` + `lstat`,
/// with 1.7% of directories accounting for 71% of the read time. Two anchors for
/// the number, neither of them precise:
///
/// - An ordinary directory read cost 0.61 ms there, so 30 s buys ~49,000 ordinary
///   directory reads inside ONE anchor subtree. Real subtrees five levels down are
///   far smaller than that.
/// - It is 2× `scanner::LOCAL_LIST_TIMEOUT`, so a subtree has to hit at least two
///   full 15 s hangs before it trips. One hung mount is not enough.
///
/// The activation counters (`reconcileBudgetSubtrees` /
/// `reconcileBudgetSkippedDirs` on the debug surface) are the instrument for
/// retuning it: if real machines trip subtrees that should have been walked, this
/// number moves, not the logic.
pub(super) const SUBTREE_READ_BUDGET: Duration = Duration::from_secs(30);

/// Depth below the volume root at which budget anchors sit.
///
/// **Provisional**, and a granularity choice rather than a measured one: an
/// anchor is the unit we refuse as a whole, so shallower anchors cover more of
/// the volume while taking more innocent directories down with a trip. Five puts
/// the anchor at app/project granularity on a boot volume (`~/Library/Caches/
/// go-build`, `~/Library/Application Support/Slack`, `~/projects-git/vdavid/cmdr`),
/// which is where the measured offenders sit.
pub(super) const ANCHOR_DEPTH: usize = 5;

/// What the walk should do with a directory it just popped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BudgetVerdict {
    /// Within budget: read it and diff it.
    Walk,
    /// Its subtree is over budget: don't descend. ❌ Not "listed and found
    /// nothing" — leave its rows and its `listed_epoch` alone.
    Skip,
}

/// One subtree's shared read-time accumulator.
struct Anchor {
    path: PathBuf,
    spent: Cell<Duration>,
    /// Set the first time `spent` crosses the budget, so one trip is reported
    /// once however many directories it later refuses.
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
    pub(super) spent: Duration,
}

/// The budget policy: where anchors sit, and what each one may spend.
pub(super) struct CostBudget {
    anchor_depth: usize,
    per_subtree: Duration,
}

impl CostBudget {
    /// The shipped policy, from [`ANCHOR_DEPTH`] and [`SUBTREE_READ_BUDGET`].
    pub(super) fn production() -> Self {
        Self::new(ANCHOR_DEPTH, SUBTREE_READ_BUDGET)
    }

    pub(super) fn new(anchor_depth: usize, per_subtree: Duration) -> Self {
        Self {
            anchor_depth,
            per_subtree,
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
                spent: Cell::new(Duration::ZERO),
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
            Some(anchor) if self.over(anchor.spent.get()) => BudgetVerdict::Skip,
            _ => BudgetVerdict::Walk,
        }
    }

    /// Charge one directory read to this directory's subtree. Returns the subtree
    /// on the read that pushes it over, and only on that read.
    pub(super) fn charge(&self, at: &Anchorage, cost: Duration) -> Option<TrippedSubtree> {
        let anchor = at.anchor.as_ref()?;
        let spent = anchor.spent.get().saturating_add(cost);
        anchor.spent.set(spent);
        if !self.over(spent) || anchor.reported.get() {
            return None;
        }
        anchor.reported.set(true);
        Some(TrippedSubtree {
            path: anchor.path.clone(),
            spent,
        })
    }

    /// A subtree that has spent EXACTLY its budget has honoured it and finishes;
    /// only spending more than it stops the descent.
    fn over(&self, spent: Duration) -> bool {
        spent > self.per_subtree
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn budget() -> CostBudget {
        CostBudget::new(1, Duration::from_millis(100))
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

    #[test]
    fn a_subtree_under_its_budget_keeps_being_walked() {
        let b = budget();
        let chain = descend(&b, &["cheap", "deep"]);
        assert!(b.charge(&chain[1], Duration::from_millis(60)).is_none());
        assert_eq!(b.verdict(&chain[2]), BudgetVerdict::Walk);
    }

    #[test]
    fn a_subtree_over_its_budget_stops_being_descended() {
        let b = budget();
        let chain = descend(&b, &["pricey", "deep"]);
        b.charge(&chain[1], Duration::from_millis(101));
        assert_eq!(b.verdict(&chain[2]), BudgetVerdict::Skip);
    }

    #[test]
    fn spending_exactly_the_budget_still_finishes() {
        let b = budget();
        let chain = descend(&b, &["exact", "deep"]);
        assert!(b.charge(&chain[1], Duration::from_millis(100)).is_none());
        assert_eq!(b.verdict(&chain[2]), BudgetVerdict::Walk);
    }

    #[test]
    fn an_over_budget_subtree_does_not_touch_its_siblings() {
        let b = budget();
        let pricey = descend(&b, &["pricey", "deep"]);
        let cheap = descend(&b, &["cheap", "deep"]);
        b.charge(&pricey[1], Duration::from_millis(500));
        assert_eq!(b.verdict(&pricey[2]), BudgetVerdict::Skip);
        assert_eq!(
            b.verdict(&cheap[2]),
            BudgetVerdict::Walk,
            "a sibling subtree is unaffected"
        );
    }

    #[test]
    fn everything_above_the_anchor_depth_is_always_walked() {
        let b = CostBudget::new(2, Duration::from_millis(100));
        let chain = descend(&b, &["a", "b"]);
        // Charging the root and the depth-1 dir is a no-op: they carry no anchor.
        assert!(b.charge(&chain[0], Duration::from_secs(60)).is_none());
        assert!(b.charge(&chain[1], Duration::from_secs(60)).is_none());
        assert_eq!(b.verdict(&chain[0]), BudgetVerdict::Walk);
        assert_eq!(b.verdict(&chain[1]), BudgetVerdict::Walk);
    }

    #[test]
    fn every_descendant_charges_the_same_anchor() {
        let b = budget();
        let chain = descend(&b, &["pricey", "deep", "deeper"]);
        // Three reads well under the budget individually, over it together.
        for level in &chain[1..] {
            b.charge(level, Duration::from_millis(40));
        }
        assert_eq!(b.verdict(&chain[1]), BudgetVerdict::Skip, "the anchor itself is over");
        assert_eq!(b.verdict(&chain[3]), BudgetVerdict::Skip, "and so is its deepest child");
    }

    #[test]
    fn a_trip_is_reported_once_with_the_subtree_and_what_it_spent() {
        let b = budget();
        let chain = descend(&b, &["pricey", "deep"]);
        assert!(b.charge(&chain[1], Duration::from_millis(60)).is_none());
        let tripped = b
            .charge(&chain[2], Duration::from_millis(60))
            .expect("the read that crosses the budget reports the subtree");
        assert_eq!(tripped.path, PathBuf::from("/vol/pricey"));
        assert_eq!(tripped.spent, Duration::from_millis(120));
        assert!(
            b.charge(&chain[2], Duration::from_millis(60)).is_none(),
            "later reads in an already-tripped subtree report nothing"
        );
    }
}

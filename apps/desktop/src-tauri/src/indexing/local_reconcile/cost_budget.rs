//! The cost backstop for the serial local reconcile walk: a cap on the SHARE of a
//! subtree's reads that may be pathologically slow, so one pathological corner of a
//! volume can't eat the whole walk.
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
//! [`SLOW_READ_PER_ENTRY_ALLOWANCE`] per entry), and a read that blows past its
//! allowance is *slow*. That per-read verdict is the whole measurement; the rule
//! below is a statistic over it.
//!
//! ## What it charges, and to whom
//!
//! Every directory read is charged to ONE accumulator: the walked directory's
//! ancestor at [`ANCHOR_DEPTH`] below the volume root (its *anchor*). Nothing above
//! the anchor depth carries a budget, so the top of the tree is always walked.
//!
//! ## The verdict is a FRACTION, never a total
//!
//! An anchor is refused when a high PROPORTION of its reads are pathological
//! ([`MAX_SLOW_READ_FRACTION`]), not when a total is exceeded. A total is the wrong
//! shape: the opportunity to accumulate one scales with subtree SIZE, so a big
//! healthy tree reaches any total eventually while a small pathological one may
//! never reach it. A fraction is size-invariant by construction. Two floors keep it
//! honest: [`MIN_SLOW_READS`] (a fraction over a handful of reads is noise) and
//! [`MIN_SLOW_TIME_WASTED`] (refusing costs a subtree's freshness, so it has to buy
//! back real time).
//!
//! ❌ Don't give every directory its own accumulator by charging each read up its
//! whole ancestor chain. A fraction needs a SAMPLE, and most directories are a
//! handful of reads; per-directory fractions would be noise that the floors would
//! then have to suppress one by one, and the unit refused would become "whichever
//! depth tripped first", which is neither predictable nor explainable. One
//! accumulator at a fixed depth gives a sample worth taking a fraction over, a
//! refusal unit that is a property of the subtree alone, and O(1) work per read.
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

/// The share of an anchor subtree's reads that may be pathological before the walk
/// stops descending into it. THE rule; the two floors below only decide when it is
/// allowed to speak.
///
/// **Measured, with two orders of magnitude to spare.** The 2026-07-21 loaded
/// reconcile (see the benchmark note linked above) put every genuinely pathological
/// subtree at ~20% slow reads (a Copilot cache at 22.6%, an Android phone at 19.8%)
/// and every healthy one at or below 1% (a pnpm store at 0.93%, David's main repo
/// at 0.10%, an Xcode SDK at 0.06%). 5% is the geometric middle of that gap: ~4×
/// clear of the noisiest healthy subtree and ~4× under the mildest pathological
/// one, so neither side needs the constant to be exactly right.
pub(super) const MAX_SLOW_READ_FRACTION: f64 = 0.05;

/// How many slow reads an anchor subtree must have seen before a fraction over them
/// counts as evidence.
///
/// **Measured.** The same walk refused `CommandLineTools/SDKs/MacOSX13.3.sdk` over
/// FOUR slow reads in 6,828 directories, so a floor of three was demonstrably too
/// low. Ten sits above every measured false positive (four) and below every
/// measured true one (14 for the Copilot cache, 18 for the phone). It doubles as
/// the floor on the sample itself, since a slow read is a read: an anchor with
/// fewer than ten reads in it can never be refused, so a three-directory subtree
/// cannot hit 33% on one bad read. A separate floor on TOTAL reads would only ever
/// delay the verdict on a small pathological subtree — the phone is 91 directories,
/// so any total-read floor big enough to matter would exempt it entirely.
pub(super) const MIN_SLOW_READS: u32 = 10;

/// How much time an anchor subtree must have LOST to slow reads before it is worth
/// refusing at all. Fast reads never count towards it.
///
/// **A judgement anchored on a measurement.** Refusing a subtree costs every
/// directory under it its freshness, so the trip should pay for itself. Five
/// seconds is more than the largest legitimate single read ever measured on this
/// machine (3.9 s for the 200,000-entry fixture), so no amount of honest work can
/// reach it, while a subtree running at the pathological ~20% has wasted enough for
/// the projected remainder to be worth stopping. It is not the discriminator —
/// every one of the five measured subtrees, right and wrong, was past 10 s — so it
/// is set low enough to let a small pathological subtree trip before it has walked
/// itself to the end.
pub(super) const MIN_SLOW_TIME_WASTED: Duration = Duration::from_secs(5);

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

/// One subtree's shared read accumulator.
struct Anchor {
    path: PathBuf,
    /// Time spent in reads that blew their allowance. Fast reads add nothing.
    slow_spent: Cell<Duration>,
    slow_reads: Cell<u32>,
    /// Every read charged to this subtree, slow or not: the denominator.
    total_reads: Cell<u32>,
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
    pub(super) total_reads: u32,
}

/// The budget policy: where anchors sit, what makes a read slow, and what share of
/// a subtree's reads may be slow before the walk gives up on it.
///
/// Every field is injected rather than read from the constants, so the walk tests
/// drive a millisecond-scale policy and retuning never touches logic. Tests build
/// one as `CostBudget { anchor_depth: 1, ..CostBudget::production() }`, which keeps
/// each test's deviation from the shipped policy visible in one line.
pub(super) struct CostBudget {
    pub(super) anchor_depth: usize,
    pub(super) fixed_allowance: Duration,
    pub(super) per_entry_allowance: Duration,
    pub(super) max_slow_fraction: f64,
    pub(super) min_slow_reads: u32,
    pub(super) min_slow_time_wasted: Duration,
}

impl CostBudget {
    /// The shipped policy, from the constants above.
    pub(super) fn production() -> Self {
        Self {
            anchor_depth: ANCHOR_DEPTH,
            fixed_allowance: SLOW_READ_FIXED_ALLOWANCE,
            per_entry_allowance: SLOW_READ_PER_ENTRY_ALLOWANCE,
            max_slow_fraction: MAX_SLOW_READ_FRACTION,
            min_slow_reads: MIN_SLOW_READS,
            min_slow_time_wasted: MIN_SLOW_TIME_WASTED,
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
                total_reads: Cell::new(0),
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

    /// Charge one directory read to this directory's subtree. Every read counts
    /// towards the denominator; only a read that blew its allowance costs the
    /// subtree time. Returns the subtree on the read that pushes it over, and only
    /// on that read.
    pub(super) fn charge(&self, at: &Anchorage, read: ReadCost) -> Option<TrippedSubtree> {
        let anchor = at.anchor.as_ref()?;
        anchor.total_reads.set(anchor.total_reads.get().saturating_add(1));
        if !self.is_slow(&read) {
            return None;
        }
        anchor
            .slow_spent
            .set(anchor.slow_spent.get().saturating_add(read.duration));
        anchor.slow_reads.set(anchor.slow_reads.get().saturating_add(1));
        // Only a slow read can push a subtree over: it is the only one that moves
        // the numerator or the clock. A fast read can only ever dilute the
        // fraction, so re-evaluating after one would never find a new verdict.
        if !self.over(anchor) || anchor.reported.get() {
            return None;
        }
        anchor.reported.set(true);
        Some(TrippedSubtree {
            path: anchor.path.clone(),
            slow_spent: anchor.slow_spent.get(),
            slow_reads: anchor.slow_reads.get(),
            total_reads: anchor.total_reads.get(),
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

    /// The verdict, evaluated on everything the subtree has shown so far: a high
    /// enough SHARE of its reads is pathological, over a big enough sample, having
    /// wasted enough time to be worth acting on. All three, or the walk carries on.
    ///
    /// Both floors are preconditions, not tiebreaks. A subtree sitting EXACTLY on a
    /// floor (or exactly on the fraction) has honoured it and keeps being walked;
    /// only passing one counts.
    fn over(&self, anchor: &Anchor) -> bool {
        let slow_reads = anchor.slow_reads.get();
        slow_reads >= self.min_slow_reads
            && anchor.slow_spent.get() > self.min_slow_time_wasted
            && f64::from(slow_reads) > self.max_slow_fraction * f64::from(anchor.total_reads.get())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A millisecond-scale policy for the boundary tests: anchors one level down, a
    /// read is slow past 10 ms + 1 ms per entry, and a subtree is refused once more
    /// than half its reads are slow, over at least two of them, having wasted more
    /// than 100 ms.
    fn budget() -> CostBudget {
        CostBudget {
            anchor_depth: 1,
            fixed_allowance: Duration::from_millis(10),
            per_entry_allowance: Duration::from_millis(1),
            max_slow_fraction: 0.5,
            min_slow_reads: 2,
            min_slow_time_wasted: Duration::from_millis(100),
        }
    }

    /// The SHIPPED policy, moved to anchor depth 1 so a test can drive it with a
    /// two-segment path. Every threshold is the production one, so the
    /// measured-subtree tests below are the real arithmetic, not a scale model.
    fn production_at_depth_1() -> CostBudget {
        CostBudget {
            anchor_depth: 1,
            ..CostBudget::production()
        }
    }

    /// An ordinary healthy read: 1 ms for ~10 entries, against a ~21 ms allowance.
    fn ordinary_read() -> ReadCost {
        read(1, 10)
    }

    /// A pathological read: `secs` seconds for three entries, hundreds of times
    /// its allowance. The shape a File Provider mount produces.
    fn pathological_read(secs: u64) -> ReadCost {
        ReadCost {
            duration: Duration::from_secs(secs),
            entries: 3,
        }
    }

    /// Charge `total` reads to `at`, of which `slow` are pathological, spread
    /// evenly through the sequence in the order the walk would meet them.
    fn charge_mixed(b: &CostBudget, at: &Anchorage, total: u32, slow: u32, slow_secs: u64) {
        let every = total / slow.max(1);
        for i in 0..total {
            if i % every == 0 && i / every < slow {
                b.charge(at, pathological_read(slow_secs));
            } else {
                b.charge(at, ordinary_read());
            }
        }
    }

    /// `~/projects-git/vdavid/cmdr`, measured at 101 slow reads in 105,441
    /// directories (0.10%). It must stay walked at any size: a rule that refuses
    /// it stops refreshing the folder David works in all day. This is the case
    /// two absolute-total budgets got wrong.
    #[test]
    fn a_subtree_with_a_low_slow_read_fraction_is_never_refused_however_large_it_grows() {
        let b = production_at_depth_1();
        let chain = descend(&b, &["cmdr", "deep"]);
        charge_mixed(&b, &chain[1], 105_441, 101, 3);
        assert_eq!(
            b.verdict(&chain[2]),
            BudgetVerdict::Walk,
            "0.10% slow reads is a healthy subtree, however much time 101 slow reads add up to"
        );
    }

    /// `~/Library/CloudStorage/MacDroid-googlePixel9ProXL`, measured at 18 slow
    /// reads in 91 directories (19.8%). Small, and genuinely pathological: size
    /// must not buy it a reprieve.
    #[test]
    fn a_small_subtree_with_a_high_slow_read_fraction_is_refused() {
        let b = production_at_depth_1();
        let chain = descend(&b, &["phone", "deep"]);
        charge_mixed(&b, &chain[1], 91, 18, 2);
        assert_eq!(
            b.verdict(&chain[2]),
            BudgetVerdict::Skip,
            "one read in five is pathological — that is a verdict on the subtree, not a hiccup"
        );
    }

    /// `CommandLineTools/SDKs/MacOSX13.3.sdk`, measured at 4 slow reads in 6,828
    /// directories (0.06%). Those four wasted 20 s between them, and refusing
    /// 6,828 directories over four unlucky reads is the sample floor's whole job.
    #[test]
    fn a_handful_of_slow_reads_in_a_huge_healthy_subtree_never_trips_it() {
        let b = production_at_depth_1();
        let chain = descend(&b, &["sdk", "deep"]);
        charge_mixed(&b, &chain[1], 6_828, 4, 5);
        assert_eq!(
            b.verdict(&chain[2]),
            BudgetVerdict::Walk,
            "four reads are not a sample, however much they cost"
        );
    }

    /// A three-directory subtree whose every read is pathological is at 100%, and
    /// still must not be refused: a fraction over three reads is noise, not
    /// evidence.
    #[test]
    fn a_fraction_over_too_small_a_sample_is_never_a_verdict() {
        let b = production_at_depth_1();
        let chain = descend(&b, &["tiny", "deep"]);
        for _ in 0..3 {
            b.charge(&chain[1], pathological_read(15));
        }
        assert_eq!(
            b.verdict(&chain[2]),
            BudgetVerdict::Walk,
            "100% of three reads is under the sample floor"
        );
    }

    /// A subtree can be pathological by proportion and still not be worth
    /// refusing: refusing costs a whole subtree's freshness, so it has to buy
    /// back real time.
    #[test]
    fn a_high_fraction_that_has_wasted_little_time_is_not_refused() {
        let b = production_at_depth_1();
        let chain = descend(&b, &["twitchy", "deep"]);
        for _ in 0..20 {
            // 100 ms for three entries: slow, but 20 of them waste only 2 s.
            b.charge(&chain[1], read(100, 3));
        }
        assert_eq!(
            b.verdict(&chain[2]),
            BudgetVerdict::Walk,
            "2 s of waste does not pay for a subtree going stale"
        );
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
        let b = CostBudget {
            min_slow_reads: 3,
            ..budget()
        };
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
        let b = CostBudget {
            anchor_depth: 2,
            min_slow_reads: 1,
            ..budget()
        };
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
        assert!(b.charge(&chain[1], read(1, 5)).is_none(), "a fast read costs nothing");
        assert!(b.charge(&chain[1], read(60, 0)).is_none());
        let tripped = b
            .charge(&chain[2], read(60, 0))
            .expect("the read that crosses the budget reports the subtree");
        assert_eq!(tripped.path, PathBuf::from("/vol/pricey"));
        assert_eq!(
            tripped.slow_spent,
            Duration::from_millis(120),
            "only the slow reads' time is on the ledger"
        );
        assert_eq!(tripped.slow_reads, 2);
        assert_eq!(tripped.total_reads, 3, "the fast read is in the denominator");
        assert!(
            b.charge(&chain[2], read(60, 0)).is_none(),
            "later reads in an already-tripped subtree report nothing"
        );
    }

    /// The heart of the fraction rule: the same three pathological reads condemn a
    /// subtree that is nothing else, and are harmless in one that is mostly
    /// healthy. Under an absolute total the 10,000 fast reads were irrelevant;
    /// here they are what saves the subtree, and no amount of growth can hurt it.
    #[test]
    fn fast_reads_dilute_the_fraction_that_slow_ones_build() {
        let b = budget();
        let mixed = descend(&b, &["mixed", "deep"]);
        let lonely = descend(&b, &["lonely", "deep"]);
        for _ in 0..10_000 {
            b.charge(&mixed[1], read(1, 5));
        }
        for _ in 0..3 {
            b.charge(&mixed[1], read(60, 0));
            b.charge(&lonely[1], read(60, 0));
        }
        assert_eq!(
            b.verdict(&mixed[2]),
            BudgetVerdict::Walk,
            "three slow reads in 10,003 is a hiccup, not a pathological subtree"
        );
        assert_eq!(
            b.verdict(&lonely[2]),
            BudgetVerdict::Skip,
            "the very same three reads, with nothing healthy around them, are a verdict"
        );
    }
}

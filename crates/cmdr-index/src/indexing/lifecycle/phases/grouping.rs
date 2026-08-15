//! How many frontier roots one `cover()` call takes.
//!
//! The machine walks a phase's frontier in groups, and this decides how big the
//! next group is. Two forces pull against each other:
//!
//! - **The queue check points.** The machine consults its visit queue BETWEEN
//!   calls, so a call's length is how long it can go without noticing the folder
//!   somebody just opened. ❌ Never one call for a whole phase's frontier: the
//!   cancel check inside `cover` is not a point a queue can be consulted at.
//! - **The per-call cost.** A call claims ground, brackets it with branch events,
//!   spawns a walk thread, and is followed by a stock-take that reads the
//!   database. Against a frontier root holding two entries, all of that is the
//!   whole cost — which is exactly the shape an interrupted run leaves behind.
//!
//! Neither side can be settled from a path: a frontier root is virgin ground by
//! definition, so nothing in the index says how big it is. So the rule is
//! measured rather than predicted: **a group is as many roots as the last group's
//! pace says fit inside the budget**, starting at one and never growing faster
//! than [`MAX_GROWTH`]. Big roots keep the group at one, which is what the
//! uninterrupted run is made of; tiny ones let it grow to [`MAX_ROOTS_PER_CALL`],
//! which is what a resumed run is made of.

use std::time::Duration;

/// How long one `cover()` call should run, so the machine comes back to its queue
/// often enough to notice where the user is looking.
///
/// A target rather than a guarantee: a single frontier root can hold a million
/// entries and no grouping rule can split it, which is why the machine keeps the
/// group at one root when roots cost that much. What this DOES guarantee is that
/// grouping never makes the wait meaningfully longer than the roots themselves
/// already do.
const INTERLEAVING_BUDGET: Duration = Duration::from_secs(1);

/// The most roots one call may take, however cheap the last ones looked.
///
/// The win is in the first few: the per-call cost is divided by the group size, so
/// 1 → 8 removes 88% of it and everything past that is rounding. The cap is what
/// bounds the damage when a group of roots that looked tiny turns out not to be.
const MAX_ROOTS_PER_CALL: usize = 16;

/// How much bigger one group may be than the group before it.
///
/// Frontier roots vary by orders of magnitude, so a single fast group is weak
/// evidence about the next one. Growing in steps means reaching the cap takes
/// several consistently cheap groups, while a slow group drops straight back to
/// one root: cheap to be wrong in the direction that costs responsiveness, slow to
/// commit in the direction that risks it.
const MAX_GROWTH: usize = 4;

/// How many frontier roots the next `cover()` call takes, from what the last one
/// cost.
pub(super) struct Grouping {
    roots: usize,
}

impl Grouping {
    /// A machine that has measured nothing yet takes one root at a time, which is
    /// the conservative answer and the one an uninterrupted run stays on.
    pub(super) fn new() -> Self {
        Self { roots: 1 }
    }

    /// How many roots to hand the next call.
    pub(super) fn roots(&self) -> usize {
        self.roots
    }

    /// Fold in what a group actually cost.
    pub(super) fn note(&mut self, roots: usize, took: Duration) {
        let roots = roots.max(1) as u32;
        let per_root = took / roots;
        let fits = if per_root.is_zero() {
            MAX_ROOTS_PER_CALL
        } else {
            usize::try_from(INTERLEAVING_BUDGET.as_nanos() / per_root.as_nanos()).unwrap_or(MAX_ROOTS_PER_CALL)
        };
        self.roots = fits.clamp(1, MAX_ROOTS_PER_CALL.min(self.roots * MAX_GROWTH));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Before anything has been measured, the machine behaves exactly as it did
    /// when one root per call was the rule.
    #[test]
    fn the_first_group_is_one_root() {
        assert_eq!(Grouping::new().roots(), 1);
    }

    /// The resumed run's shape: roots that cost almost nothing, so the per-call
    /// cost is the whole cost and the group grows to the cap.
    #[test]
    fn roots_that_cost_nothing_grow_the_group_to_the_cap() {
        let mut grouping = Grouping::new();
        for _ in 0..10 {
            grouping.note(grouping.roots(), Duration::from_millis(2) * grouping.roots() as u32);
        }
        assert_eq!(grouping.roots(), MAX_ROOTS_PER_CALL);
    }

    /// And it takes several cheap groups to get there, so one unrepresentative
    /// measurement can't commit the machine to a long call.
    #[test]
    fn the_group_grows_in_steps_rather_than_in_one_jump() {
        let mut grouping = Grouping::new();
        grouping.note(1, Duration::from_micros(1));
        assert_eq!(
            grouping.roots(),
            MAX_GROWTH,
            "one cheap root buys one step, not the cap"
        );
    }

    /// The uninterrupted run's shape, and the constraint that matters most: one
    /// root that ate the whole budget by itself puts the machine straight back to
    /// one root per call, so the visit queue keeps being consulted between them.
    #[test]
    fn a_root_that_outran_the_budget_on_its_own_goes_back_to_one_root() {
        let mut grouping = Grouping::new();
        for _ in 0..4 {
            grouping.note(grouping.roots(), Duration::from_micros(1));
        }
        let grown = grouping.roots();
        assert!(grown > 1, "a run of cheap roots grew the group");

        grouping.note(grown, INTERLEAVING_BUDGET * grown as u32 * 3);
        assert_eq!(grouping.roots(), 1);
    }

    /// A group that overran the budget is resized to what its OWN pace says fits,
    /// rather than to one: the roots were affordable, there were too many of them,
    /// and dropping to one root would give back the whole win over the tiny roots a
    /// resumed run is made of.
    #[test]
    fn a_group_that_overran_is_resized_to_what_its_pace_fits() {
        let mut grouping = Grouping::new();
        grouping.note(16, INTERLEAVING_BUDGET * 4);
        assert_eq!(grouping.roots(), 4, "a quarter of a second per root fits four");
    }

    /// A group is sized from the PER-ROOT cost, not the group's: eight roots
    /// costing a second between them are eight roots' worth of budget, and the
    /// machine that just walked them may take eight again.
    #[test]
    fn the_pace_is_measured_per_root_rather_than_per_group() {
        let mut grouping = Grouping::new();
        grouping.note(8, INTERLEAVING_BUDGET);
        assert_eq!(
            grouping.roots(),
            4,
            "the budget fits eight, the growth step allows four"
        );

        let mut grouping = Grouping::new();
        grouping.note(8, INTERLEAVING_BUDGET * 8);
        assert_eq!(grouping.roots(), 1, "a second per root is one root per call");
    }

    /// Whatever the arithmetic says, a call always takes at least one root and
    /// never more than the cap — the two ways a sizing rule can wedge a machine.
    #[test]
    fn a_group_is_never_empty_and_never_unbounded() {
        let mut grouping = Grouping::new();
        for took in [Duration::ZERO, Duration::from_secs(600), Duration::from_nanos(1)] {
            grouping.note(0, took);
            assert!((1..=MAX_ROOTS_PER_CALL).contains(&grouping.roots()));
        }
    }
}

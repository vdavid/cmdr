//! What the steps still ahead of a whole-volume run took last time.
//!
//! The overall "~X left" a host shows while a drive indexes is the active step's
//! live estimate plus this: for each step, the remembered durations of every step
//! AFTER it on this kind of run. This module owns that sum and its honesty gate;
//! the host only adds its live step estimate to the entry for the active step.
//!
//! ❌ Never a guess: one step ahead with no history of this walk kind makes the
//! whole remainder `None`, so the host shows no overall figure. A drive's first
//! index, a run covered in phases, and an event-log roll-on carry no plan at all
//! (`StepsAhead::default()`).

use crate::indexing::store::StepDurations;

/// Which pipeline a whole-volume run goes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunShape {
    /// The local walker: find files, save the file list, compute folder sizes,
    /// catch up on the changes that arrived meanwhile.
    Local,
    /// A `Volume`-trait walk: entries land inline as the walk goes, and no
    /// catch-up pass follows, so it's find files, then compute folder sizes.
    Network,
}

/// The remembered time left AFTER each step finishes, in milliseconds. `None`
/// where this run's history can't honestly say, and for a step this run's shape
/// doesn't have.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct StepsAhead {
    pub after_find_files_ms: Option<u64>,
    pub after_save_ms: Option<u64>,
    pub after_compute_ms: Option<u64>,
    pub after_catch_up_ms: Option<u64>,
}

impl StepsAhead {
    /// Sum the remembered durations of the steps after each one, for a run of
    /// this shape. The walk itself is never ahead of anything, so its own
    /// duration plays no part.
    pub(crate) fn remembered(shape: RunShape, durations: StepDurations) -> Self {
        // `?` on each term is the honesty gate: one unknown step ahead and the
        // whole remainder is unknown.
        let sum = |steps: &[Option<u64>]| {
            steps
                .iter()
                .try_fold(0u64, |total, step| Some(total.saturating_add((*step)?)))
        };
        let StepDurations {
            save_ms,
            compute_ms,
            catch_up_ms,
        } = durations;
        match shape {
            RunShape::Local => Self {
                after_find_files_ms: sum(&[save_ms, compute_ms, catch_up_ms]),
                after_save_ms: sum(&[compute_ms, catch_up_ms]),
                after_compute_ms: sum(&[catch_up_ms]),
                after_catch_up_ms: Some(0),
            },
            RunShape::Network => Self {
                after_find_files_ms: sum(&[compute_ms]),
                after_save_ms: None,
                after_compute_ms: Some(0),
                after_catch_up_ms: None,
            },
        }
    }
}

#[cfg(test)]
mod tests;

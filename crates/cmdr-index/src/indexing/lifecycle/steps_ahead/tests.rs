//! The remembered remainder behind the overall "~X left": which steps count as
//! ahead for each run shape, and the gate that keeps a missing history from
//! becoming a made-up figure.

use super::*;

fn all_known() -> StepDurations {
    StepDurations {
        save_ms: Some(40_000),
        compute_ms: Some(19_000),
        catch_up_ms: Some(2_000),
    }
}

#[test]
fn a_local_run_sums_every_step_after_each_one() {
    let ahead = StepsAhead::remembered(RunShape::Local, all_known());
    assert_eq!(
        ahead,
        StepsAhead {
            after_find_files_ms: Some(61_000),
            after_save_ms: Some(21_000),
            after_compute_ms: Some(2_000),
            after_catch_up_ms: Some(0),
        }
    );
}

#[test]
fn a_network_run_has_only_the_compute_step_ahead_of_the_walk() {
    // No save step (entries land inline) and no catch-up pass, so neither may
    // add to the figure, and neither gets an entry of its own.
    let ahead = StepsAhead::remembered(RunShape::Network, all_known());
    assert_eq!(
        ahead,
        StepsAhead {
            after_find_files_ms: Some(19_000),
            after_save_ms: None,
            after_compute_ms: Some(0),
            after_catch_up_ms: None,
        }
    );
}

#[test]
fn one_step_without_history_hides_every_remainder_that_includes_it() {
    // The catch-up step never finished on this kind of run before. Every step it
    // follows can't say how long is left; the last step itself still can.
    let durations = StepDurations {
        catch_up_ms: None,
        ..all_known()
    };
    let ahead = StepsAhead::remembered(RunShape::Local, durations);
    assert_eq!(ahead.after_find_files_ms, None);
    assert_eq!(ahead.after_save_ms, None);
    assert_eq!(ahead.after_compute_ms, None);
    assert_eq!(ahead.after_catch_up_ms, Some(0));
}

#[test]
fn a_gap_early_in_the_pipeline_leaves_the_later_remainders_intact() {
    // Save has no history, but it's behind compute: once save is done, the rest
    // is known again.
    let durations = StepDurations {
        save_ms: None,
        ..all_known()
    };
    let ahead = StepsAhead::remembered(RunShape::Local, durations);
    assert_eq!(ahead.after_find_files_ms, None);
    assert_eq!(ahead.after_save_ms, Some(21_000));
    assert_eq!(ahead.after_compute_ms, Some(2_000));
}

#[test]
fn a_network_run_ignores_the_steps_it_never_runs() {
    // A network walk never saves separately or catches up, so their missing
    // history mustn't hide its figure.
    let durations = StepDurations {
        save_ms: None,
        compute_ms: Some(19_000),
        catch_up_ms: None,
    };
    let ahead = StepsAhead::remembered(RunShape::Network, durations);
    assert_eq!(ahead.after_find_files_ms, Some(19_000));
}

#[test]
fn nothing_remembered_means_no_plan_past_the_last_step() {
    let ahead = StepsAhead::remembered(RunShape::Local, StepDurations::default());
    assert_eq!(ahead.after_find_files_ms, None);
    assert_eq!(ahead.after_save_ms, None);
    assert_eq!(ahead.after_compute_ms, None);
    assert_eq!(ahead.after_catch_up_ms, Some(0));
}

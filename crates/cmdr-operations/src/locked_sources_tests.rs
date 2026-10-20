//! The question's own contract: who it's asked of, what an answer does to the
//! plan, and what a cancel while asking reports. What each move engine does with
//! the plan: `transfer/metadata_matrix_tests.rs` and the volume move suites.

use std::sync::Arc;
use std::time::Duration;

use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::testing::TestOperations;
use cmdr_fs::ignore_poison::IgnorePoison;

fn state(engine: &TestOperations) -> Arc<WriteOperationState> {
    Arc::new(engine.new_operation_state(Duration::from_millis(50)))
}

fn scan(locked: &[&str], system_locked: &[&str]) -> LockedScan {
    LockedScan {
        locked: locked.iter().map(PathBuf::from).collect(),
        system_locked: system_locked.iter().map(PathBuf::from).collect(),
    }
}

#[test]
fn nothing_locked_asks_nobody_and_plans_nothing() {
    let engine = TestOperations::new();
    let events = CollectorEventSink::new();

    let plan = plan_locked_local(&events, "op", WriteOperationType::Move, &state(&engine), scan(&[], &[]))
        .expect("nothing to cancel");

    assert!(events.locked_sources.lock_ignore_poison().is_empty());
    assert!(!plan.skips_any() && !plan.unlocks(Path::new("/a")));
    assert_eq!(plan.kept(), None, "nothing stays, so the completion says nothing");
}

#[test]
fn a_system_locked_item_is_skipped_without_asking() {
    let engine = TestOperations::new();
    let events = CollectorEventSink::new();

    let plan = plan_locked_local(
        &events,
        "op",
        WriteOperationType::Move,
        &state(&engine),
        scan(&[], &["/s"]),
    )
    .expect("nothing to cancel");

    assert!(events.locked_sources.lock_ignore_poison().is_empty());
    assert!(plan.skips(Path::new("/s")) && plan.skips_under(Path::new("/s/child")));
    assert_eq!(
        plan.kept(),
        Some(LockedKept {
            skipped: 0,
            system_locked: 1
        })
    );
}

#[test]
fn move_anyway_unlocks_the_locked_items_and_still_skips_the_system_ones() {
    let engine = TestOperations::new();
    let state = state(&engine);
    let events = CollectorEventSink::new();
    let answering = Arc::clone(&state);
    events.answer_locked_with(move |_| {
        assert!(answering.locked_sources.answer(LockedSourcesAnswer::MoveAnyway));
    });

    let plan = plan_locked_local(
        &events,
        "op",
        WriteOperationType::Move,
        &state,
        scan(&["/a", "/b"], &["/s"]),
    )
    .expect("answered");

    let asked = events.locked_sources.lock_ignore_poison().clone();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].locked_count, 2, "only the Locked items are asked about");
    assert_eq!(asked[0].locked_sample, vec!["/a".to_string(), "/b".to_string()]);
    assert!(plan.unlocks(Path::new("/a")) && plan.unlocks(Path::new("/b")));
    assert!(!plan.skips(Path::new("/a")) && plan.skips(Path::new("/s")));
    assert_eq!(state.locked_sources.pending(), None, "the question is spent");
}

#[test]
fn skip_keeps_the_locked_items_and_counts_them() {
    let engine = TestOperations::new();
    let state = state(&engine);
    let events = CollectorEventSink::new();
    let answering = Arc::clone(&state);
    events.answer_locked_with(move |_| {
        answering.locked_sources.answer(LockedSourcesAnswer::Skip);
    });

    let plan =
        plan_locked_local(&events, "op", WriteOperationType::Move, &state, scan(&["/a"], &[])).expect("answered");

    assert!(plan.skips(Path::new("/a")) && !plan.unlocks(Path::new("/a")));
    assert_eq!(
        plan.kept(),
        Some(LockedKept {
            skipped: 1,
            system_locked: 0
        })
    );
}

#[test]
fn a_cancel_while_asking_ends_the_move_with_its_cancel_event() {
    let engine = TestOperations::new();
    let state = state(&engine);
    let events = CollectorEventSink::new();
    let cancelling = Arc::clone(&state);
    events.answer_locked_with(move |_| cancelling.locked_sources.abandon());

    let plan = plan_locked_local(&events, "op", WriteOperationType::Move, &state, scan(&["/a"], &[]));

    assert!(plan.is_none());
    let cancelled = events.cancelled.lock_ignore_poison().clone();
    assert_eq!(cancelled.len(), 1);
    assert_eq!(cancelled[0].files_processed, 0);
}

#[test]
fn the_question_is_answered_through_the_registered_operation() {
    let engine = TestOperations::new();
    assert_eq!(
        engine.answer_locked_sources(&OperationId::from("never-registered"), LockedSourcesAnswer::Skip),
        LockedSourcesAnswerOutcome::UnknownOperation
    );
    assert_eq!(
        engine.pending_locked_sources(&OperationId::from("never-registered")),
        None
    );
}

#[test]
fn an_anchored_plan_answers_a_volumes_relative_paths() {
    let plan = settle(scan(&["/root/a"], &["/root/s"]), Some(LockedSourcesAnswer::MoveAnyway))
        .anchored_at(PathBuf::from("/root"));

    assert!(plan.unlocks(Path::new("a")) && plan.unlocks(Path::new("/root/a")));
    assert!(plan.skips_under(Path::new("s/inner")));
}

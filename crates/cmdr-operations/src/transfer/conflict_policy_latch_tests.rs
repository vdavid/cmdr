//! A clash answered with "… all" changes the policy the operation reports
//! (`conflict_policy_cell.rs`), on the local engine. The volume engine's twin is
//! `volume/conflict_policy_latch_tests.rs`.

use std::fs;
use std::sync::Arc;
use std::time::Duration;

use super::super::state::WriteOperationState;
use super::super::types::{ConflictResolution, WriteOperationConfig};
use super::conflict_responder_test_support::ConflictResponderSink;
use super::copy::copy_files_with_progress_inner;
use crate::testing::TestOperations;
use cmdr_fs::testing::TestDir;

/// A copy asking for each clash, over a destination that already holds the file.
fn run_ask_each_copy(answer: ConflictResolution, apply_to_all: bool) -> Arc<WriteOperationState> {
    let engine = TestOperations::new();
    let dir = TestDir::new(&format!("conflict_policy_latch_{}", uuid::Uuid::new_v4()));
    let (src_root, dst_root) = (dir.join("src"), dir.join("dst"));
    fs::create_dir_all(&src_root).unwrap();
    fs::create_dir_all(&dst_root).unwrap();
    fs::write(src_root.join("notes.txt"), "new").unwrap();
    fs::write(dst_root.join("notes.txt"), "old").unwrap();

    let state = Arc::new(
        engine
            .new_operation_state(Duration::from_millis(0))
            .with_conflict_policy(ConflictResolution::Stop),
    );
    let events = ConflictResponderSink::new(&state, answer, apply_to_all);
    copy_files_with_progress_inner(
        &events,
        "op-policy-latch",
        &state,
        &[src_root.join("notes.txt")],
        &dst_root,
        &WriteOperationConfig {
            conflict_resolution: ConflictResolution::Stop,
            ..Default::default()
        },
    )
    .expect("the copy should finish once the clash is answered");
    state
}

#[test]
fn an_overwrite_all_answer_becomes_the_reported_policy() {
    let state = run_ask_each_copy(ConflictResolution::Overwrite, true);
    assert_eq!(state.conflict_policy.current(), Some(ConflictResolution::Overwrite));
}

#[test]
fn a_one_off_answer_leaves_the_policy_on_ask_for_each() {
    let state = run_ask_each_copy(ConflictResolution::Overwrite, false);
    assert_eq!(
        state.conflict_policy.current(),
        Some(ConflictResolution::Stop),
        "answering one clash decides nothing about the next"
    );
}
